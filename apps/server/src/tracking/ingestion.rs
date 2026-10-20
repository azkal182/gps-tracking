use super::super::*;
use super::heartbeat::take_rate_token;
use serde_json::{Map, Value};
use std::time::Instant;

const MAX_POINTS: usize = 100;
const MAX_ATTRIBUTES_BYTES: usize = 4096;
const MAX_FUTURE_SKEW_SECONDS: i64 = 120;
const MAX_LATE_ARRIVAL_SECONDS: i64 = 24 * 60 * 60;

#[track_caller]
fn database_failure(error: sqlx::Error) -> StatusCode {
    #[cfg(test)]
    eprintln!(
        "TRACK-02 integration database failure at {}: {error}",
        std::panic::Location::caller()
    );
    let _ = error;
    StatusCode::INTERNAL_SERVER_ERROR
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionBatchRequest {
    device_id: String,
    points: Vec<PositionInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionInput {
    message_id: String,
    latitude: f64,
    longitude: f64,
    altitude_m: Option<f64>,
    speed_mps: Option<f64>,
    heading_deg: Option<f64>,
    accuracy_m: Option<f64>,
    recorded_at: String,
    attributes: Option<Value>,
}

#[derive(Serialize)]
struct PositionBatchResponse {
    device_id: String,
    summary: PositionBatchSummary,
    results: Vec<PositionResult>,
}

#[derive(Serialize)]
struct PositionBatchSummary {
    received: usize,
    stored: usize,
    duplicate: usize,
    rejected: usize,
}

#[derive(Serialize)]
struct PositionResult {
    index: usize,
    message_id: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retryable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry_after_ms: Option<i64>,
}

#[derive(Serialize)]
struct CanonicalPosition<'a> {
    message_id: &'a str,
    latitude: f64,
    longitude: f64,
    altitude_m: Option<f64>,
    speed_mps: Option<f64>,
    heading_deg: Option<f64>,
    accuracy_m: Option<f64>,
    recorded_at: &'a str,
    attributes: &'a Value,
}

fn canonical_float(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

struct NormalizedPosition {
    input: PositionInput,
    recorded_at: String,
    attributes: Value,
    fingerprint: [u8; 32],
}

fn valid_rfc3339_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return false;
    }
    let zone_start = if value.ends_with('Z') || value.ends_with('z') {
        value.len() - 1
    } else if value.len() >= 6
        && matches!(bytes[value.len() - 6], b'+' | b'-')
        && bytes[value.len() - 3] == b':'
    {
        value.len() - 6
    } else {
        return false;
    };
    if zone_start < 19 || (zone_start > 19 && bytes[19] != b'.') {
        return false;
    }
    bytes[..19]
        .iter()
        .enumerate()
        .all(|(index, byte)| matches!(index, 4 | 7 | 10 | 13 | 16) || byte.is_ascii_digit())
        && (zone_start == 19
            || bytes[20..zone_start]
                .iter()
                .all(|byte| byte.is_ascii_digit()))
}

fn validate_structure(request: &PositionBatchRequest) -> Result<(), StatusCode> {
    if uuid::Uuid::parse_str(&request.device_id).is_err()
        || request.points.is_empty()
        || request.points.len() > MAX_POINTS
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    for point in &request.points {
        if point.message_id.is_empty()
            || point.message_id.len() > 128
            || !point.latitude.is_finite()
            || !point.longitude.is_finite()
            || !(-90.0..=90.0).contains(&point.latitude)
            || !(-180.0..=180.0).contains(&point.longitude)
            || !valid_rfc3339_shape(&point.recorded_at)
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        for value in [
            point.altitude_m,
            point.speed_mps,
            point.heading_deg,
            point.accuracy_m,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
        if point.speed_mps.is_some_and(|value| value < 0.0)
            || point
                .heading_deg
                .is_some_and(|value| !(0.0..360.0).contains(&value))
            || point.accuracy_m.is_some_and(|value| value < 0.0)
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        if let Some(attributes) = &point.attributes {
            if !attributes.is_object()
                || serde_json::to_vec(attributes)
                    .map_or(true, |value| value.len() > MAX_ATTRIBUTES_BYTES)
            {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
        }
    }
    Ok(())
}

async fn normalize_times(
    pool: &PgPool,
    points: Vec<PositionInput>,
) -> Result<Vec<NormalizedPosition>, StatusCode> {
    let timestamps: Vec<String> = points
        .iter()
        .map(|point| point.recorded_at.clone())
        .collect();
    let normalized: Vec<String> = sqlx::query_scalar(
        "SELECT to_char(value::timestamptz AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') \
         FROM unnest($1::text[]) AS values(value)",
    )
    .bind(&timestamps)
    .fetch_all(pool)
    .await
    .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    if normalized.len() != points.len() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    points
        .into_iter()
        .zip(normalized)
        .map(|(input, recorded_at)| {
            let attributes = input
                .attributes
                .clone()
                .unwrap_or_else(|| Value::Object(Map::new()));
            let canonical = CanonicalPosition {
                message_id: &input.message_id,
                latitude: canonical_float(input.latitude),
                longitude: canonical_float(input.longitude),
                altitude_m: input.altitude_m.map(canonical_float),
                speed_mps: input.speed_mps.map(canonical_float),
                heading_deg: input.heading_deg.map(canonical_float),
                accuracy_m: input.accuracy_m.map(canonical_float),
                recorded_at: &recorded_at,
                attributes: &attributes,
            };
            let bytes =
                serde_json::to_vec(&canonical).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
            let fingerprint: [u8; 32] = Sha256::digest(bytes).into();
            Ok(NormalizedPosition {
                input,
                recorded_at,
                attributes,
                fingerprint,
            })
        })
        .collect()
}

fn result(
    index: usize,
    message_id: &str,
    status: &'static str,
    position_id: Option<String>,
    reason: Option<&'static str>,
    retry: Option<i64>,
) -> PositionResult {
    PositionResult {
        index,
        message_id: message_id.to_owned(),
        status,
        position_id,
        reason,
        retryable: retry.map(|_| true),
        retry_after_ms: retry,
    }
}

pub(crate) async fn ingest_positions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PositionBatchRequest>,
) -> Result<Response, StatusCode> {
    validate_structure(&request)?;
    let pool = state
        .tracking_pool
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let auth = headers
        .get(header::AUTHORIZATION)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let auth = auth.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
    let token = auth
        .strip_prefix("Bearer ")
        .filter(|value| value.starts_with("gps_live_"))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token_hash = digest(token);
    let identity = tracking_storage::lookup_device_credential(&pool, &token_hash)
        .await
        .map_err(database_failure)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if let Err(retry_after) = take_rate_token(
        &state.position_rate_limit,
        &identity.device_id,
        Instant::now(),
        1.0,
        10.0,
    ) {
        let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
        if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after.to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        return Ok(response);
    }
    if uuid::Uuid::parse_str(&request.device_id)
        .ok()
        .map(|id| id.to_string())
        .as_deref()
        != Some(identity.device_id.as_str())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut points = normalize_times(&pool, request.points).await?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &identity.tenant_id)
        .await
        .map_err(database_failure)?;
    let received_at: String = sqlx::query_scalar(
        "SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(database_failure)?;
    let previous_latest: Option<String> = sqlx::query_scalar("SELECT id::text FROM positions WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND expires_at>clock_timestamp() ORDER BY recorded_at DESC,id DESC LIMIT 1")
        .bind(&identity.tenant_id).bind(&identity.device_id).fetch_optional(&mut *tx).await.map_err(database_failure)?;
    let session_rows = sqlx::query_as::<_, (String, String)>(
        "SELECT id::text,target_id::text FROM tracking_sessions WHERE tenant_id=$1::uuid AND device_id=$2::uuid \
         AND started_at <= $3::timestamptz AND (ended_at IS NULL OR ended_at >= $4::timestamptz)",
    )
    .bind(&identity.tenant_id).bind(&identity.device_id)
    .bind(points.iter().map(|point| point.recorded_at.as_str()).max().unwrap())
    .bind(points.iter().map(|point| point.recorded_at.as_str()).min().unwrap())
    .fetch_all(&mut *tx).await.map_err(database_failure)?;
    let target_ids: Vec<String> = session_rows.iter().map(|row| row.1.clone()).collect();
    let session_ids: Vec<String> = session_rows.iter().map(|row| row.0.clone()).collect();
    tracking_storage::lock_tracking_resources(
        &mut tx,
        &identity.tenant_id,
        std::slice::from_ref(&identity.device_id),
        &target_ids,
        &session_ids,
    )
    .await
    .map_err(database_failure)?;
    let refreshed = tracking_storage::lookup_device_credential(&pool, &token_hash)
        .await
        .map_err(database_failure)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if refreshed.device_id != identity.device_id || refreshed.tenant_id != identity.tenant_id {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let identity_policy = refreshed.ingestion_policy;
    let mut results = Vec::with_capacity(points.len());
    let mut stored = 0;
    let mut duplicate = 0;
    let mut rejected = 0;
    for (index, point) in points.drain(..).enumerate() {
        let existing = sqlx::query_as::<_, (String, Vec<u8>)>(
            "SELECT id::text,payload_fingerprint FROM positions WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND message_id=$3",
        ).bind(&identity.tenant_id).bind(&identity.device_id).bind(&point.input.message_id)
        .fetch_optional(&mut *tx).await.map_err(database_failure)?;
        if let Some((position_id, fingerprint)) = existing {
            if fingerprint.as_slice() == point.fingerprint {
                duplicate += 1;
                results.push(result(
                    index,
                    &point.input.message_id,
                    "duplicate",
                    Some(position_id),
                    None,
                    None,
                ));
            } else {
                let request_id = headers
                    .get("x-request-id")
                    .and_then(|value| value.to_str().ok())
                    .filter(|value| !value.is_empty() && value.len() <= 128)
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                sqlx::query(
                    "INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) \
                     VALUES($1::uuid,$2,'position.message_id_conflict','device',$3::uuid,'denied',$4)",
                )
                .bind(&identity.tenant_id)
                .bind(format!("device:{}", identity.device_id))
                .bind(&identity.device_id)
                .bind(request_id)
                .execute(&mut *tx)
                .await
                .map_err(database_failure)?;
                rejected += 1;
                results.push(result(
                    index,
                    &point.input.message_id,
                    "rejected",
                    None,
                    Some("MESSAGE_ID_CONFLICT"),
                    None,
                ));
            }
            continue;
        }
        let time_delta_ms: i64 = sqlx::query_scalar(
            "SELECT ceil(extract(epoch FROM ($1::timestamptz-$2::timestamptz))*1000)::bigint",
        )
        .bind(&point.recorded_at)
        .bind(&received_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_failure)?;
        let reason = if time_delta_ms > MAX_FUTURE_SKEW_SECONDS * 1000 {
            Some("TIMESTAMP_IN_FUTURE")
        } else if time_delta_ms < -MAX_LATE_ARRIVAL_SECONDS * 1000 {
            Some("TIMESTAMP_TOO_OLD")
        } else if time_delta_ms > 0 && identity_policy == "session_only" {
            let retry = time_delta_ms;
            rejected += 1;
            results.push(result(
                index,
                &point.input.message_id,
                "rejected",
                None,
                Some("TIMESTAMP_NOT_READY"),
                Some(retry),
            ));
            continue;
        } else {
            None
        };
        if let Some(reason) = reason {
            rejected += 1;
            results.push(result(
                index,
                &point.input.message_id,
                "rejected",
                None,
                Some(reason),
                None,
            ));
            continue;
        }
        if identity_policy == "session_only" {
            let in_session: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM tracking_sessions WHERE tenant_id=$1::uuid AND device_id=$2::uuid \
                 AND started_at <= $3::timestamptz AND (ended_at IS NULL OR $3::timestamptz < ended_at))",
            ).bind(&identity.tenant_id).bind(&identity.device_id).bind(&point.recorded_at)
                .fetch_one(&mut *tx).await.map_err(database_failure)?;
            if !in_session {
                rejected += 1;
                results.push(result(
                    index,
                    &point.input.message_id,
                    "rejected",
                    None,
                    Some("OUTSIDE_SESSION"),
                    None,
                ));
                continue;
            }
        }
        let inserted = sqlx::query_scalar::<_, String>(
            "INSERT INTO positions(tenant_id,device_id,message_id,payload_fingerprint,location,altitude_m,speed_mps,heading_deg,accuracy_m,recorded_at,received_at,attributes,expires_at) \
             VALUES($1::uuid,$2::uuid,$3,$4,ST_SetSRID(ST_MakePoint($5,$6),4326)::geography,$7,$8,$9,$10,$11::timestamptz,$12::timestamptz,$13::jsonb,position_expires_at_utc_days($11::timestamptz,$14)) RETURNING id::text",
        ).bind(&identity.tenant_id).bind(&identity.device_id).bind(&point.input.message_id).bind(point.fingerprint.to_vec())
            .bind(point.input.longitude).bind(point.input.latitude).bind(point.input.altitude_m).bind(point.input.speed_mps)
            .bind(point.input.heading_deg).bind(point.input.accuracy_m).bind(&point.recorded_at).bind(&received_at)
            .bind(point.attributes).bind(state.position_retention_days).fetch_one(&mut *tx).await.map_err(database_failure)?;
        stored += 1;
        results.push(result(
            index,
            &point.input.message_id,
            "stored",
            Some(inserted),
            None,
            None,
        ));
    }
    sqlx::query(
        "UPDATE devices SET last_contact_at=GREATEST(COALESCE(last_contact_at,$3::timestamptz),$3::timestamptz) \
         WHERE tenant_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&identity.tenant_id)
    .bind(&identity.device_id)
    .bind(&received_at)
    .execute(&mut *tx)
    .await
    .map_err(database_failure)?;
    let latest: Option<(String,String,String,f64,f64)> = sqlx::query_as("SELECT id::text,to_char(recorded_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),to_char(received_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),ST_Y(location::geometry),ST_X(location::geometry) FROM positions WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND expires_at>clock_timestamp() ORDER BY recorded_at DESC,id DESC LIMIT 1")
        .bind(&identity.tenant_id).bind(&identity.device_id).fetch_optional(&mut *tx).await.map_err(database_failure)?;
    let latest_context = if let Some((_, recorded_at, _, _, _)) = latest.as_ref() {
        sqlx::query_as::<_, (String,String)>("SELECT id::text,target_id::text FROM tracking_sessions WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND ended_at IS NULL AND started_at<=$3::timestamptz LIMIT 1")
            .bind(&identity.tenant_id).bind(&identity.device_id).bind(recorded_at).fetch_optional(&mut *tx).await.map_err(database_failure)?
    } else {
        None
    };
    tx.commit().await.map_err(database_failure)?;
    state
        .metrics
        .record_position_results(stored, duplicate, rejected);
    if let Some((position_id, recorded_at, point_received_at, latitude, longitude)) =
        latest.filter(|row| previous_latest.as_deref() != Some(row.0.as_str()))
    {
        let (target_id, session_id) = latest_context
            .map(|(session, target)| (Some(target), Some(session)))
            .unwrap_or((None, None));
        let data = serde_json::json!({"device_id":identity.device_id,"target_id":target_id,"session_id":session_id,"position":{"id":position_id,"latitude":latitude,"longitude":longitude,"recorded_at":recorded_at,"received_at":point_received_at}});
        publish_event(
            &state,
            &identity.tenant_id,
            Some(&identity.device_id),
            target_id.as_deref(),
            session_id.as_deref(),
            "position.updated",
            &received_at,
            data,
        );
    }
    Ok((
        StatusCode::OK,
        Json(PositionBatchResponse {
            device_id: identity.device_id,
            summary: PositionBatchSummary {
                received: results.len(),
                stored,
                duplicate,
                rejected,
            },
            results,
        }),
    )
        .into_response())
}
