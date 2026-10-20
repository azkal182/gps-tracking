use super::super::*;
use axum::http::HeaderValue;
use std::time::Instant;

const HEARTBEAT_RATE_PER_SECOND: f64 = 6.0 / 60.0;
const HEARTBEAT_BURST: f64 = 3.0;
const HEARTBEAT_BUCKET_CAPACITY: usize = 50_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HeartbeatRequest {}

#[derive(Serialize)]
struct HeartbeatResponse {
    device_id: String,
    last_contact_at: String,
    connectivity_status: &'static str,
}

fn take_heartbeat_token(
    buckets: &Mutex<HashMap<String, HeartbeatRateBucket>>,
    device_id: &str,
    now: Instant,
) -> Result<(), u64> {
    take_rate_token(
        buckets,
        device_id,
        now,
        HEARTBEAT_RATE_PER_SECOND,
        HEARTBEAT_BURST,
    )
}

pub(super) fn take_rate_token(
    buckets: &Mutex<HashMap<String, HeartbeatRateBucket>>,
    device_id: &str,
    now: Instant,
    rate_per_second: f64,
    burst: f64,
) -> Result<(), u64> {
    let Ok(mut buckets) = buckets.lock() else {
        return Err(1);
    };
    buckets.retain(|_, bucket| now.duration_since(bucket.updated_at) < Duration::from_secs(3600));
    if !buckets.contains_key(device_id) && buckets.len() >= HEARTBEAT_BUCKET_CAPACITY {
        return Err(1);
    }
    let bucket = buckets
        .entry(device_id.to_owned())
        .or_insert(HeartbeatRateBucket {
            tokens: burst,
            updated_at: now,
        });
    let elapsed = now.duration_since(bucket.updated_at).as_secs_f64();
    bucket.tokens = (bucket.tokens + elapsed * rate_per_second).min(burst);
    bucket.updated_at = now;
    if bucket.tokens >= 1.0 {
        bucket.tokens -= 1.0;
        Ok(())
    } else {
        Err(((1.0 - bucket.tokens) / rate_per_second).ceil() as u64)
    }
}

fn request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

fn response_headers(response: &mut Response, request_id: &str) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
}

pub(crate) async fn ingest_heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<HeartbeatRequest>,
) -> Result<Response, StatusCode> {
    let pool = state
        .tracking_pool
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let authorization = headers
        .get(header::AUTHORIZATION)
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_str()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let token = authorization
        .strip_prefix("Bearer ")
        .filter(|value| value.starts_with("gps_live_"))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token_hash = digest(token);
    let identity = tracking_storage::lookup_device_credential(&pool, &token_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if let Err(retry_seconds) = take_heartbeat_token(
        &state.heartbeat_rate_limit,
        &identity.device_id,
        Instant::now(),
    ) {
        let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
        if let Ok(value) = HeaderValue::from_str(&retry_seconds.to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response_headers(&mut response, &request_id(&headers));
        return Ok(response);
    }

    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &identity.tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tracking_storage::lock_tracking_resources(
        &mut tx,
        &identity.tenant_id,
        std::slice::from_ref(&identity.device_id),
        &[],
        &[],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let refreshed = tracking_storage::lookup_device_credential(&pool, &token_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if refreshed.device_id != identity.device_id || refreshed.tenant_id != identity.tenant_id {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let last_contact_at = sqlx::query_scalar::<_, String>(
        "WITH contact AS (SELECT clock_timestamp() AS at) \
         UPDATE devices d SET last_contact_at=GREATEST(COALESCE(d.last_contact_at,contact.at),contact.at) \
         FROM contact WHERE d.tenant_id=$1::uuid AND d.id=$2::uuid \
         RETURNING to_char(d.last_contact_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')",
    )
    .bind(&identity.tenant_id)
    .bind(&identity.device_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::UNAUTHORIZED)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut response = (
        StatusCode::OK,
        Json(HeartbeatResponse {
            device_id: identity.device_id,
            last_contact_at,
            connectivity_status: "online",
        }),
    )
        .into_response();
    response_headers(&mut response, &request_id(&headers));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_limiter_allows_three_burst_then_refills_six_per_minute() {
        let limiter = Mutex::new(HashMap::new());
        let now = Instant::now();
        for _ in 0..3 {
            assert!(take_heartbeat_token(&limiter, "device-a", now).is_ok());
        }
        assert_eq!(take_heartbeat_token(&limiter, "device-a", now), Err(10));
        assert!(take_heartbeat_token(&limiter, "device-a", now + Duration::from_secs(10)).is_ok());
        assert!(take_heartbeat_token(&limiter, "device-b", now).is_ok());
    }

    #[test]
    fn position_limiter_allows_ten_burst_then_one_per_second() {
        let limiter = Mutex::new(HashMap::new());
        let now = Instant::now();
        for _ in 0..10 {
            assert!(take_rate_token(&limiter, "device-a", now, 1.0, 10.0).is_ok());
        }
        assert_eq!(
            take_rate_token(&limiter, "device-a", now, 1.0, 10.0),
            Err(1)
        );
        assert!(
            take_rate_token(
                &limiter,
                "device-a",
                now + Duration::from_secs(1),
                1.0,
                10.0
            )
            .is_ok()
        );
    }

    #[test]
    fn heartbeat_body_is_empty_and_rejects_unknown_fields() {
        assert!(serde_json::from_str::<HeartbeatRequest>("{}").is_ok());
        assert!(serde_json::from_str::<HeartbeatRequest>(r#"{"latitude":0}"#).is_err());
    }
}
