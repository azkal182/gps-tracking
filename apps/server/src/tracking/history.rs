use super::super::*;
use super::sessions::{SessionActor, normalized_time, request_actor, valid_uuid};
use axum::{
    extract::{Path, Query},
    http::HeaderValue,
};

const MAX_HISTORY_DAYS: i64 = 31;
const DEFAULT_HISTORY_LIMIT: u16 = 100;
const MAX_HISTORY_LIMIT: u16 = 1000;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct PositionHistoryQuery {
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) limit: Option<u16>,
    pub(crate) cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionCursor {
    scope: String,
    from: String,
    to: String,
    recorded_at: String,
    id: String,
}

#[derive(Serialize)]
struct PositionPage {
    data: Vec<PositionDto>,
    next_cursor: Option<String>,
}

#[derive(Clone, Serialize)]
struct PositionDto {
    id: String,
    device_id: String,
    message_id: Option<String>,
    latitude: f64,
    longitude: f64,
    altitude_m: Option<f64>,
    speed_mps: Option<f64>,
    heading_deg: Option<f64>,
    accuracy_m: Option<f64>,
    recorded_at: String,
    received_at: String,
    attributes: serde_json::Value,
    tenant_id: String,
}

#[derive(Serialize)]
struct DeviceLocation {
    device_id: String,
    connectivity_status: String,
    position_status: &'static str,
    position: Option<PositionDto>,
    last_contact_at: Option<String>,
    tenant_id: String,
    observed_at: String,
}

#[derive(Serialize)]
struct TargetLocation {
    target_id: String,
    tracking_status: &'static str,
    connectivity_status: Option<String>,
    position_status: &'static str,
    session_id: Option<String>,
    device_id: Option<String>,
    position: Option<PositionDto>,
    last_contact_at: Option<String>,
    tenant_id: String,
    observed_at: String,
}

fn json_response<T: Serialize>(status: StatusCode, request_id: &str, body: T) -> Response {
    let mut response = (status, Json(body)).into_response();
    add_no_store_request_id(&mut response, request_id);
    response
}

fn add_no_store_request_id(response: &mut Response, request_id: &str) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
}

fn position_cursor_scope(tenant: &str, kind: &str, resource_id: &str) -> String {
    format!("position-history-v1|{tenant}|{kind}|{resource_id}|recorded_at_desc_id_desc")
}

fn encode_position_cursor(
    key: &[u8; 32],
    scope: &str,
    from: &str,
    to: &str,
    recorded_at: &str,
    id: &str,
) -> Option<String> {
    let claims = serde_json::to_vec(&PositionCursor {
        scope: scope.to_owned(),
        from: from.to_owned(),
        to: to.to_owned(),
        recorded_at: recorded_at.to_owned(),
        id: id.to_owned(),
    })
    .ok()?;
    let mut mac = CursorMac::new_from_slice(key).ok()?;
    mac.update(&claims);
    let mut signed = claims;
    signed.extend_from_slice(&mac.finalize().into_bytes());
    Some(URL_SAFE_NO_PAD.encode(signed))
}

fn decode_position_cursor(
    key: &[u8; 32],
    scope: &str,
    encoded: &str,
) -> Result<PositionCursor, StatusCode> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    if bytes.len() <= 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (claims, signature) = bytes.split_at(bytes.len() - 32);
    let mut mac = CursorMac::new_from_slice(key).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(claims);
    mac.verify_slice(signature)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let cursor: PositionCursor =
        serde_json::from_slice(claims).map_err(|_| StatusCode::BAD_REQUEST)?;
    if cursor.scope != scope || !cursor.id.parse::<i64>().is_ok_and(|id| id > 0) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(cursor)
}

async fn history_range(
    pool: &PgPool,
    query: &PositionHistoryQuery,
    cursor: Option<&PositionCursor>,
) -> Result<(String, String), StatusCode> {
    let requested_from = normalized_time(pool, query.from.as_deref()).await?;
    let requested_to = normalized_time(pool, query.to.as_deref()).await?;
    if let Some(cursor) = cursor {
        if requested_from
            .as_ref()
            .is_some_and(|value| value != &cursor.from)
            || requested_to
                .as_ref()
                .is_some_and(|value| value != &cursor.to)
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        validate_history_range(pool, &cursor.from, &cursor.to).await?;
        return Ok((cursor.from.clone(), cursor.to.clone()));
    }

    let to = if let Some(to) = requested_to {
        to
    } else {
        sqlx::query_scalar::<_, String>(
            "SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')",
        )
        .fetch_one(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };
    let from = if let Some(from) = requested_from {
        from
    } else {
        sqlx::query_scalar::<_, String>(
            "SELECT to_char(($1::timestamptz-interval '24 hours') AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')",
        )
        .bind(&to)
        .fetch_one(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };
    validate_history_range(pool, &from, &to).await?;
    Ok((from, to))
}

async fn validate_history_range(pool: &PgPool, from: &str, to: &str) -> Result<(), StatusCode> {
    let valid: bool = sqlx::query_scalar(
        "SELECT $1::timestamptz < $2::timestamptz AND $2::timestamptz-$1::timestamptz <= ($3::double precision * interval '1 day')",
    )
    .bind(from)
    .bind(to)
    .bind(MAX_HISTORY_DAYS)
    .fetch_one(pool)
    .await
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    if valid {
        Ok(())
    } else {
        Err(StatusCode::UNPROCESSABLE_ENTITY)
    }
}

fn history_limit(value: Option<u16>) -> Result<i64, StatusCode> {
    let limit = value.unwrap_or(DEFAULT_HISTORY_LIMIT);
    if !(1..=MAX_HISTORY_LIMIT).contains(&limit) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(i64::from(limit))
}

async fn require_position_permission(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &SessionActor,
    tenant: &str,
    resource_type: &str,
    resource_id: &str,
) -> Result<(), StatusCode> {
    let allowed: bool = match actor {
        SessionActor::Portal { user_id } => sqlx::query_scalar(
            "SELECT gps_tracking_member_has_permission($1::uuid,$2::uuid,$3,$4::uuid,'read')",
        )
        .bind(tenant)
        .bind(user_id)
        .bind(resource_type)
        .bind(resource_id)
        .fetch_one(&mut **tx)
        .await,
        SessionActor::Application { token_id, .. } => sqlx::query_scalar(
            "SELECT gps_tracking_application_token_owner_has_position_permission($1::uuid,$2::uuid,$3,$4::uuid)",
        )
        .bind(token_id)
        .bind(tenant)
        .bind(resource_type)
        .bind(resource_id)
        .fetch_one(&mut **tx)
        .await,
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn position_dto(row: PositionRow) -> PositionDto {
    PositionDto {
        id: row.id,
        device_id: row.device_id,
        message_id: row.message_id,
        latitude: row.latitude,
        longitude: row.longitude,
        altitude_m: row.altitude_m,
        speed_mps: row.speed_mps,
        heading_deg: row.heading_deg,
        accuracy_m: row.accuracy_m,
        recorded_at: row.recorded_at,
        received_at: row.received_at,
        attributes: row.attributes,
        tenant_id: row.tenant_id,
    }
}

struct PositionRow {
    id: String,
    device_id: String,
    message_id: Option<String>,
    latitude: f64,
    longitude: f64,
    altitude_m: Option<f64>,
    speed_mps: Option<f64>,
    heading_deg: Option<f64>,
    accuracy_m: Option<f64>,
    recorded_at: String,
    received_at: String,
    attributes: serde_json::Value,
    tenant_id: String,
}

type PositionRowFields = (
    String,
    String,
    Option<String>,
    f64,
    f64,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    String,
    String,
    serde_json::Value,
    String,
);

fn into_position_row(row: PositionRowFields) -> PositionRow {
    PositionRow {
        id: row.0,
        device_id: row.1,
        message_id: row.2,
        latitude: row.3,
        longitude: row.4,
        altitude_m: row.5,
        speed_mps: row.6,
        heading_deg: row.7,
        accuracy_m: row.8,
        recorded_at: row.9,
        received_at: row.10,
        attributes: row.11,
        tenant_id: row.12,
    }
}

const POSITION_SELECT: &str = "p.id::text,p.device_id::text,p.message_id,ST_Y(p.location::geometry),ST_X(p.location::geometry),p.altitude_m,p.speed_mps,p.heading_deg,p.accuracy_m,to_char(p.recorded_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),to_char(p.received_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),p.attributes,p.tenant_id::text";

pub(crate) async fn get_device_location(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (pool, actor, tenant) = request_actor(&state, &headers, false).await?;
    actor.require_scope("positions:read")?;
    let device_id = valid_uuid(&raw_id)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    require_position_permission(&mut tx, &actor, &tenant, "device", &device_id).await?;
    let row = sqlx::query_as::<_, (Option<String>, String, String)>(
        "SELECT CASE WHEN d.last_contact_at IS NULL THEN NULL ELSE to_char(d.last_contact_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') END, \
             CASE WHEN d.last_contact_at IS NULL THEN 'unknown' WHEN GREATEST(0,extract(epoch FROM (snapshot.at-d.last_contact_at))) < $3 THEN 'online' ELSE 'offline' END, \
             to_char(snapshot.at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') \
             FROM devices d CROSS JOIN (SELECT clock_timestamp() AS at) snapshot \
             WHERE d.tenant_id=$1::uuid AND d.id=$2::uuid"
    )
    .bind(&tenant)
    .bind(&device_id)
    .bind(state.device_offline_timeout_seconds as f64)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let position = sqlx::query_as::<_, PositionRowFields>(&format!(
        "SELECT {POSITION_SELECT} FROM positions p WHERE p.tenant_id=$1::uuid AND p.device_id=$2::uuid AND p.expires_at>clock_timestamp() ORDER BY p.recorded_at DESC,p.id DESC LIMIT 1"
    ))
    .bind(&tenant)
    .bind(&device_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map(into_position_row);
    let position_status = if let Some(position) = &position {
        let is_fresh: bool = sqlx::query_scalar(
            "SELECT GREATEST(0,extract(epoch FROM ($1::timestamptz-$2::timestamptz))) < $3",
        )
        .bind(&row.2)
        .bind(&position.recorded_at)
        .bind(state.position_freshness_seconds as f64)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if is_fresh { "fresh" } else { "stale" }
    } else {
        "no_position"
    };
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(json_response(
        StatusCode::OK,
        &request_id,
        DeviceLocation {
            device_id,
            connectivity_status: row.1,
            position_status,
            position: position.map(position_dto),
            last_contact_at: row.0,
            tenant_id: tenant,
            observed_at: row.2.clone(),
        },
    ))
}

pub(crate) async fn get_target_location(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (pool, actor, tenant) = request_actor(&state, &headers, false).await?;
    actor.require_scope("positions:read")?;
    let target_id = valid_uuid(&raw_id)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    require_position_permission(&mut tx, &actor, &tenant, "target", &target_id).await?;
    let session = sqlx::query_as::<_, (String, String)>(
        "SELECT id::text,device_id::text FROM tracking_sessions WHERE tenant_id=$1::uuid AND target_id=$2::uuid AND ended_at IS NULL FOR SHARE",
    )
    .bind(&tenant)
    .bind(&target_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some((session_id, device_id)) = session else {
        let observed_at: String = sqlx::query_scalar(
            "SELECT to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(json_response(
            StatusCode::OK,
            &request_id,
            TargetLocation {
                target_id,
                tracking_status: "inactive",
                connectivity_status: None,
                position_status: "no_position",
                session_id: None,
                device_id: None,
                position: None,
                last_contact_at: None,
                tenant_id: tenant,
                observed_at,
            },
        ));
    };
    require_position_permission(&mut tx, &actor, &tenant, "device", &device_id).await?;
    let row = sqlx::query_as::<_, (Option<String>, String, String)>(
        "SELECT CASE WHEN d.last_contact_at IS NULL THEN NULL ELSE to_char(d.last_contact_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') END, \
         CASE WHEN d.last_contact_at IS NULL THEN 'unknown' WHEN GREATEST(0,extract(epoch FROM (snapshot.at-d.last_contact_at))) < $3 THEN 'online' ELSE 'offline' END, \
         to_char(snapshot.at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') \
         FROM devices d CROSS JOIN (SELECT clock_timestamp() AS at) snapshot WHERE d.tenant_id=$1::uuid AND d.id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&device_id)
    .bind(state.device_offline_timeout_seconds as f64)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let position = sqlx::query_as::<_, PositionRowFields>(&format!(
        "SELECT {POSITION_SELECT} FROM positions p JOIN tracking_sessions s ON s.tenant_id=p.tenant_id AND s.device_id=p.device_id \
         WHERE p.tenant_id=$1::uuid AND p.device_id=$2::uuid AND s.id=$3::uuid AND p.recorded_at >= s.started_at \
         AND (s.ended_at IS NULL OR p.recorded_at < s.ended_at) AND p.expires_at>clock_timestamp() \
         ORDER BY p.recorded_at DESC,p.id DESC LIMIT 1"
    ))
    .bind(&tenant)
    .bind(&device_id)
    .bind(&session_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map(into_position_row);
    let position_status = if let Some(position) = &position {
        let is_fresh: bool = sqlx::query_scalar(
            "SELECT GREATEST(0,extract(epoch FROM ($1::timestamptz-$2::timestamptz))) < $3",
        )
        .bind(&row.2)
        .bind(&position.recorded_at)
        .bind(state.position_freshness_seconds as f64)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if is_fresh { "fresh" } else { "stale" }
    } else {
        "no_position"
    };
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(json_response(
        StatusCode::OK,
        &request_id,
        TargetLocation {
            target_id,
            tracking_status: "active",
            connectivity_status: Some(row.1),
            position_status,
            session_id: Some(session_id),
            device_id: Some(device_id),
            position: position.map(position_dto),
            last_contact_at: row.0,
            tenant_id: tenant,
            observed_at: row.2,
        },
    ))
}

struct HistoryContext {
    state: AppState,
    actor: SessionActor,
    tenant: String,
    pool: PgPool,
}

async fn history_page(
    context: HistoryContext,
    resource_kind: &str,
    resource_id: &str,
    query: PositionHistoryQuery,
    session_id: Option<String>,
) -> Result<Response, StatusCode> {
    let HistoryContext {
        state,
        actor,
        tenant,
        pool,
    } = context;
    actor.require_scope("positions:read")?;
    let limit = history_limit(query.limit)?;
    let base_scope = position_cursor_scope(&tenant, resource_kind, resource_id);
    let key = state
        .auth
        .as_ref()
        .and_then(|auth| auth.mfa_key.as_ref())
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| decode_position_cursor(key, &base_scope, value))
        .transpose()?;
    let (from, to) = history_range(&pool, &query, cursor.as_ref()).await?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let session = if let Some(session_id) = session_id.as_deref() {
        let session = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
            "SELECT id::text,target_id::text,device_id::text,to_char(started_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),CASE WHEN ended_at IS NULL THEN NULL ELSE to_char(ended_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') END FROM tracking_sessions WHERE tenant_id=$1::uuid AND id=$2::uuid FOR SHARE",
        )
        .bind(&tenant)
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
        require_position_permission(&mut tx, &actor, &tenant, "target", &session.1).await?;
        require_position_permission(&mut tx, &actor, &tenant, "device", &session.2).await?;
        Some(session)
    } else {
        require_position_permission(&mut tx, &actor, &tenant, resource_kind, resource_id).await?;
        None
    };
    let after_at = cursor.as_ref().map(|value| value.recorded_at.as_str());
    let after_id = cursor.as_ref().map(|value| value.id.as_str());
    let rows = if let Some((_, _, device_id, started_at, ended_at)) = session.as_ref() {
        sqlx::query_as::<_, PositionRowFields>(&format!(
            "SELECT {POSITION_SELECT} FROM positions p WHERE p.tenant_id=$1::uuid AND p.device_id=$2::uuid \
             AND p.recorded_at >= $3::timestamptz AND p.recorded_at < $4::timestamptz AND p.expires_at>clock_timestamp() \
             AND p.recorded_at >= $5::timestamptz AND ($6::timestamptz IS NULL OR p.recorded_at < $6::timestamptz) \
             AND ($7::timestamptz IS NULL OR (p.recorded_at,p.id)<($7::timestamptz,$8::bigint)) \
             ORDER BY p.recorded_at DESC,p.id DESC LIMIT $9"
        ))
        .bind(&tenant)
        .bind(device_id)
        .bind(&from)
        .bind(&to)
        .bind(started_at)
        .bind(ended_at.as_deref())
        .bind(after_at)
        .bind(after_id)
        .bind(limit + 1)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        sqlx::query_as::<_, PositionRowFields>(&format!(
            "SELECT {POSITION_SELECT} FROM positions p WHERE p.tenant_id=$1::uuid AND p.device_id=$2::uuid \
             AND p.recorded_at >= $3::timestamptz AND p.recorded_at < $4::timestamptz AND p.expires_at>clock_timestamp() \
             AND ($5::timestamptz IS NULL OR (p.recorded_at,p.id)<($5::timestamptz,$6::bigint)) \
             ORDER BY p.recorded_at DESC,p.id DESC LIMIT $7"
        ))
        .bind(&tenant)
        .bind(resource_id)
        .bind(&from)
        .bind(&to)
        .bind(after_at)
        .bind(after_id)
        .bind(limit + 1)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut rows = rows;
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let row = rows.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(
            encode_position_cursor(key, &base_scope, &from, &to, &row.9, &row.0)
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    Ok(json_response(
        StatusCode::OK,
        &uuid::Uuid::new_v4().to_string(),
        PositionPage {
            data: rows
                .into_iter()
                .map(into_position_row)
                .map(position_dto)
                .collect(),
            next_cursor,
        },
    ))
}

pub(crate) async fn get_device_positions(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
    Query(query): Query<PositionHistoryQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (pool, actor, tenant) = request_actor(&state, &headers, false).await?;
    let device_id = valid_uuid(&raw_id)?;
    history_page(
        HistoryContext {
            state,
            actor,
            tenant,
            pool,
        },
        "device",
        &device_id,
        query,
        None,
    )
    .await
}

pub(crate) async fn get_session_positions(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
    Query(query): Query<PositionHistoryQuery>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (pool, actor, tenant) = request_actor(&state, &headers, false).await?;
    actor.require_scope("positions:read")?;
    let session_id = valid_uuid(&raw_id)?;
    history_page(
        HistoryContext {
            state,
            actor,
            tenant,
            pool,
        },
        "session",
        &session_id.clone(),
        query,
        Some(session_id),
    )
    .await
}
