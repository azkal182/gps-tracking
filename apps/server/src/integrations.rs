use super::*;
use axum::{extract::Path, http::HeaderValue};
use std::collections::HashSet;

const APPLICATION_TOKEN_SCOPES: &[&str] = &[
    "targets:read",
    "targets:write",
    "devices:read",
    "devices:write",
    "sessions:read",
    "sessions:write",
    "positions:read",
    "tracking:subscribe",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ApplicationTokenRequest {
    pub(super) name: String,
    pub(super) scopes: Vec<String>,
    pub(super) device_ids: Vec<String>,
    pub(super) target_ids: Vec<String>,
}

#[derive(Serialize)]
pub(super) struct ApplicationTokenMetadata {
    id: String,
    tenant_id: String,
    name: String,
    scopes: Vec<String>,
    device_ids: Vec<String>,
    target_ids: Vec<String>,
    expires_at: String,
    created_at: String,
}

#[derive(Serialize)]
pub(super) struct ApplicationTokenIssued {
    id: String,
    tenant_id: String,
    name: String,
    scopes: Vec<String>,
    device_ids: Vec<String>,
    target_ids: Vec<String>,
    expires_at: String,
    created_at: String,
    token: String,
}

#[derive(Serialize)]
pub(super) struct ApplicationTokenPage {
    data: Vec<ApplicationTokenMetadata>,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct AuditQuery {
    #[serde(rename = "from")]
    pub(super) from_time: Option<String>,
    #[serde(rename = "to")]
    pub(super) to_time: Option<String>,
    pub(super) actor_id: Option<String>,
    pub(super) action: Option<String>,
    pub(super) resource_id: Option<String>,
    pub(super) limit: Option<u16>,
    pub(super) cursor: Option<String>,
}

#[derive(Serialize)]
pub(super) struct AuditEvent {
    id: String,
    tenant_id: String,
    actor_id: Option<String>,
    action: String,
    resource_type: Option<String>,
    resource_id: Option<String>,
    outcome: String,
    request_id: String,
    occurred_at: String,
}

#[derive(Serialize)]
pub(super) struct AuditPage {
    data: Vec<AuditEvent>,
    next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditCursor {
    scope: String,
    from_time: String,
    to_time: String,
    occurred_at: String,
    id: String,
}

pub(super) async fn issue_application_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ApplicationTokenRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    require_recent_auth(&auth, &user, true).await?;
    let pool = state.tracking_pool.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 150 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let scope_set = body.scopes.iter().collect::<HashSet<_>>();
    if body.scopes.is_empty()
        || scope_set.len() != body.scopes.len()
        || body
            .scopes
            .iter()
            .any(|scope| !APPLICATION_TOKEN_SCOPES.contains(&scope.as_str()))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    if body.device_ids.is_empty() && body.target_ids.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let device_ids = canonical_ids(&body.device_ids)?;
    let target_ids = canonical_ids(&body.target_ids)?;
    let token_id = uuid::Uuid::new_v4().to_string();
    let request_id = uuid::Uuid::new_v4().to_string();
    let token = format!("gps_app_{}", random_secret());
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tracking_storage::lock_tracking_resources(&mut tx, &tenant, &device_ids, &target_ids, &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    require_current_tenant_admin(&mut tx, &tenant, &user.user_id).await?;
    if !tenant_resources_exist(&mut tx, &tenant, "device", &device_ids).await?
        || !tenant_resources_exist(&mut tx, &tenant, "target", &target_ids).await?
    {
        return Err(StatusCode::NOT_FOUND);
    }
    let (created_at, expires_at) = sqlx::query_as::<_, (String, String)>(
        "INSERT INTO application_tokens(id,tenant_id,owner_user_id,name,token_hash,scopes,expires_at) \
         VALUES($1::uuid,$2::uuid,$3::uuid,$4,$5,$6,clock_timestamp()+interval '90 days') \
         RETURNING to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), \
         to_char(expires_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')",
    )
    .bind(&token_id)
    .bind(&tenant)
    .bind(&user.user_id)
    .bind(name)
    .bind(digest(&token))
    .bind(&body.scopes)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    for device_id in &device_ids {
        sqlx::query("INSERT INTO application_token_devices(tenant_id,application_token_id,device_id) VALUES($1::uuid,$2::uuid,$3::uuid)")
            .bind(&tenant).bind(&token_id).bind(device_id).execute(&mut *tx).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    for target_id in &target_ids {
        sqlx::query("INSERT INTO application_token_targets(tenant_id,application_token_id,target_id) VALUES($1::uuid,$2::uuid,$3::uuid)")
            .bind(&tenant).bind(&token_id).bind(target_id).execute(&mut *tx).await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    insert_audit_event(
        &mut tx,
        &tenant,
        &user.user_id,
        "application_token.created",
        Some("application_token"),
        Some(&token_id),
        &request_id,
    )
    .await?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = ApplicationTokenIssued {
        id: token_id,
        tenant_id: tenant,
        name: name.to_owned(),
        scopes: body.scopes,
        device_ids,
        target_ids,
        expires_at,
        created_at,
        token,
    };
    Ok(json_response(StatusCode::CREATED, &request_id, response))
}

pub(super) async fn list_application_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    let pool = state.tracking_pool.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let limit = page_limit(&query)?;
    let scope = format!("application-tokens:{tenant}:active:created_at_desc_id_desc");
    let key = auth
        .mfa_key
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| decode_page_cursor(key, &scope, value))
        .transpose()?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tracking_storage::lock_tracking_resources(&mut tx, &tenant, &[], &[], &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    require_current_tenant_admin(&mut tx, &tenant, &user.user_id).await?;
    let rows = sqlx::query_as::<_, (String, String, Vec<String>, Vec<String>, Vec<String>, String, String)>(
        "SELECT t.id::text,t.name,t.scopes, \
         COALESCE((SELECT array_agg(a.device_id::text ORDER BY a.device_id) FROM application_token_devices a WHERE a.tenant_id=t.tenant_id AND a.application_token_id=t.id),ARRAY[]::text[]), \
         COALESCE((SELECT array_agg(a.target_id::text ORDER BY a.target_id) FROM application_token_targets a WHERE a.tenant_id=t.tenant_id AND a.application_token_id=t.id),ARRAY[]::text[]), \
         to_char(t.expires_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), \
         to_char(t.created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') \
         FROM application_tokens t WHERE t.tenant_id=$1::uuid AND t.revoked_at IS NULL \
         AND ($2::timestamptz IS NULL OR (t.created_at,t.id)<($2::timestamptz,$3::uuid)) \
         ORDER BY t.created_at DESC,t.id DESC LIMIT $4",
    )
    .bind(&tenant)
    .bind(cursor.as_ref().map(|value| value.created_at.as_str()))
    .bind(cursor.as_ref().map(|value| value.id.as_str()))
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
            encode_page_cursor(key, &scope, &row.6, &row.0)
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    let data = rows
        .into_iter()
        .map(
            |(id, name, scopes, device_ids, target_ids, expires_at, created_at)| {
                ApplicationTokenMetadata {
                    id,
                    tenant_id: tenant.clone(),
                    name,
                    scopes,
                    device_ids,
                    target_ids,
                    expires_at,
                    created_at,
                }
            },
        )
        .collect();
    let request_id = uuid::Uuid::new_v4().to_string();
    Ok(json_response(
        StatusCode::OK,
        &request_id,
        ApplicationTokenPage { data, next_cursor },
    ))
}

pub(super) async fn revoke_application_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    let token_id = uuid::Uuid::parse_str(&id)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_string();
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    let pool = state.tracking_pool.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tracking_storage::lock_tracking_resources(&mut tx, &tenant, &[], &[], &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    require_current_tenant_admin(&mut tx, &tenant, &user.user_id).await?;
    let changed = sqlx::query("UPDATE application_tokens SET revoked_at=clock_timestamp() WHERE tenant_id=$1::uuid AND id=$2::uuid AND revoked_at IS NULL")
        .bind(&tenant).bind(&token_id).execute(&mut *tx).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.rows_affected() == 1;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM application_tokens WHERE tenant_id=$1::uuid AND id=$2::uuid)",
    )
    .bind(&tenant)
    .bind(&token_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    if changed {
        insert_audit_event(
            &mut tx,
            &tenant,
            &user.user_id,
            "application_token.revoked",
            Some("application_token"),
            Some(&token_id),
            &request_id,
        )
        .await?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(empty_response(StatusCode::NO_CONTENT, &request_id))
}

pub(super) async fn list_audit_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditQuery>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    let pool = state.tracking_pool.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let limit = page_limit(&PageQuery {
        limit: query.limit,
        cursor: query.cursor.clone(),
    })?;
    let actor_id = optional_uuid(query.actor_id.as_deref())?;
    let resource_id = optional_uuid(query.resource_id.as_deref())?;
    let action = query
        .action
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    if query
        .action
        .as_deref()
        .is_some_and(|v| v.trim().is_empty() || v.trim().len() > 128)
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let scope = serde_json::to_string(&(
        "audit-events-v1",
        tenant.as_str(),
        query.from_time.as_deref(),
        query.to_time.as_deref(),
        actor_id.as_deref(),
        action,
        resource_id.as_deref(),
        "occurred_at_desc_id_desc",
    ))
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let key = auth
        .mfa_key
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| decode_audit_cursor(key, &scope, value))
        .transpose()?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tracking_storage::lock_tracking_resources(&mut tx, &tenant, &[], &[], &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    require_current_tenant_admin(&mut tx, &tenant, &user.user_id).await?;
    let (from_time, to_time) = if let Some(cursor) = &cursor {
        (cursor.from_time.clone(), cursor.to_time.clone())
    } else {
        audit_bounds(
            &mut tx,
            query.from_time.as_deref(),
            query.to_time.as_deref(),
        )
        .await?
    };
    let valid_range: bool = sqlx::query_scalar(
        "SELECT $1::timestamptz < $2::timestamptz AND $2::timestamptz <= $1::timestamptz + interval '31 days'",
    )
    .bind(&from_time)
    .bind(&to_time)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid_range {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let rows = sqlx::query_as::<_, (String, String, Option<String>, String, Option<String>, Option<String>, String, String, String)>(
        "SELECT e.id::text,e.tenant_id::text, \
         CASE WHEN e.actor_ref ~ '^user:[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$' THEN substring(e.actor_ref FROM 6)::uuid::text ELSE NULL END, \
         e.action,e.resource_type,e.resource_id::text,e.outcome,e.request_id,to_char(e.occurred_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') \
         FROM audit_events e WHERE e.tenant_id=$1::uuid AND e.occurred_at >= $2::timestamptz AND e.occurred_at < $3::timestamptz \
         AND ($4::uuid IS NULL OR e.actor_ref='user:'||$4::text) \
         AND ($5::text IS NULL OR e.action=$5) AND ($6::uuid IS NULL OR e.resource_id=$6::uuid) \
         AND ($7::timestamptz IS NULL OR (e.occurred_at,e.id)<($7::timestamptz,$8::bigint)) \
         ORDER BY e.occurred_at DESC,e.id DESC LIMIT $9",
    )
    .bind(&tenant)
    .bind(&from_time)
    .bind(&to_time)
    .bind(actor_id.as_deref())
    .bind(action)
    .bind(resource_id.as_deref())
    .bind(cursor.as_ref().map(|value| value.occurred_at.as_str()))
    .bind(cursor.as_ref().map(|value| value.id.as_str()))
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut rows = rows;
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let row = rows.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(encode_audit_cursor(
            key, &scope, &from_time, &to_time, &row.8, &row.0,
        )?)
    } else {
        None
    };
    let data = rows
        .into_iter()
        .map(
            |(
                id,
                tenant_id,
                actor_id,
                action,
                resource_type,
                resource_id,
                outcome,
                request_id,
                occurred_at,
            )| {
                AuditEvent {
                    id,
                    tenant_id,
                    actor_id,
                    action,
                    resource_type,
                    resource_id,
                    outcome,
                    request_id,
                    occurred_at,
                }
            },
        )
        .collect::<Vec<_>>();
    insert_audit_event(
        &mut tx,
        &tenant,
        &user.user_id,
        "audit.events.read",
        None,
        None,
        &request_id,
    )
    .await?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(json_response(
        StatusCode::OK,
        &request_id,
        AuditPage { data, next_cursor },
    ))
}

fn canonical_ids(values: &[String]) -> Result<Vec<String>, StatusCode> {
    let mut ids = Vec::with_capacity(values.len());
    for value in values {
        ids.push(
            uuid::Uuid::parse_str(value)
                .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?
                .to_string(),
        );
    }
    if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(ids)
}

fn optional_uuid(value: Option<&str>) -> Result<Option<String>, StatusCode> {
    value
        .map(|id| {
            uuid::Uuid::parse_str(id)
                .map(|parsed| parsed.to_string())
                .map_err(|_| StatusCode::BAD_REQUEST)
        })
        .transpose()
}

async fn require_current_tenant_admin(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    user_id: &str,
) -> Result<(), StatusCode> {
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM memberships m JOIN organizations o ON o.id=m.tenant_id \
         WHERE m.tenant_id=$1::uuid AND m.user_id=$2::uuid \
         AND m.role='organization_admin' AND m.status='active' AND o.status='active' \
         AND public.gps_tracking_member_has_permission($1::uuid,$2::uuid,'organization',$1::uuid,'targets:create'))",
    )
    .bind(tenant)
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

async fn tenant_resources_exist(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    resource: &str,
    ids: &[String],
) -> Result<bool, StatusCode> {
    let table = match resource {
        "device" => "devices",
        "target" => "targets",
        _ => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    if ids.is_empty() {
        return Ok(true);
    }
    let query = format!(
        "SELECT count(*) FROM {table} WHERE tenant_id=$1::uuid AND id::text=ANY($2::text[])"
    );
    let count: i64 = sqlx::query_scalar(&query)
        .bind(tenant)
        .bind(ids)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(count == ids.len() as i64)
}

async fn audit_bounds(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    from_time: Option<&str>,
    to_time: Option<&str>,
) -> Result<(String, String), StatusCode> {
    sqlx::query_as::<_, (String, String)>(
        "WITH args AS (SELECT $1::timestamptz AS from_at,$2::timestamptz AS to_at,clock_timestamp() AS now_at) \
         SELECT to_char((CASE WHEN from_at IS NOT NULL THEN from_at WHEN to_at IS NOT NULL THEN to_at-interval '31 days' ELSE now_at-interval '31 days' END) AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), \
         to_char((CASE WHEN to_at IS NOT NULL THEN to_at WHEN from_at IS NOT NULL THEN from_at+interval '31 days' ELSE now_at END) AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') FROM args",
    )
    .bind(from_time)
    .bind(to_time)
    .fetch_one(&mut **tx)
    .await
    .map_err(map_audit_time_error)
}

fn map_audit_time_error(error: sqlx::Error) -> StatusCode {
    if let Some(database) = error.as_database_error()
        && matches!(
            database.code().as_deref(),
            Some("22007" | "22008" | "22009")
        )
    {
        return StatusCode::BAD_REQUEST;
    }
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn insert_audit_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    user_id: &str,
    action: &str,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    request_id: &str,
) -> Result<(), StatusCode> {
    sqlx::query(
        "INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) \
         VALUES($1::uuid,$2,$3,$4,$5::uuid,'success',$6)",
    )
    .bind(tenant)
    .bind(format!("user:{user_id}"))
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(request_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(())
}

fn json_response<T: Serialize>(status: StatusCode, request_id: &str, value: T) -> Response {
    let mut response = (status, Json(value)).into_response();
    add_response_headers(&mut response, request_id);
    response
}

fn empty_response(status: StatusCode, request_id: &str) -> Response {
    let mut response = status.into_response();
    add_response_headers(&mut response, request_id);
    response
}

fn add_response_headers(response: &mut Response, request_id: &str) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
}

fn encode_audit_cursor(
    key: &[u8; 32],
    scope: &str,
    from_time: &str,
    to_time: &str,
    occurred_at: &str,
    id: &str,
) -> Result<String, StatusCode> {
    let cursor = AuditCursor {
        scope: scope.to_owned(),
        from_time: from_time.to_owned(),
        to_time: to_time.to_owned(),
        occurred_at: occurred_at.to_owned(),
        id: id.to_owned(),
    };
    let claims = serde_json::to_vec(&cursor).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut mac = CursorMac::new_from_slice(key).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    mac.update(&claims);
    let mut signed = claims;
    signed.extend_from_slice(&mac.finalize().into_bytes());
    Ok(URL_SAFE_NO_PAD.encode(signed))
}

fn decode_audit_cursor(
    key: &[u8; 32],
    scope: &str,
    encoded: &str,
) -> Result<AuditCursor, StatusCode> {
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
    let cursor: AuditCursor =
        serde_json::from_slice(claims).map_err(|_| StatusCode::BAD_REQUEST)?;
    if cursor.scope != scope || !cursor.id.parse::<i64>().is_ok_and(|id| id > 0) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(cursor)
}
