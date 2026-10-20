use super::super::*;
use axum::{extract::Path, http::HeaderValue};

const SESSION_ROUTE: &str = "POST /v1/sessions";

#[derive(Clone)]
pub(super) enum SessionActor {
    Portal {
        user_id: String,
    },
    Application {
        token_id: String,
        scopes: Vec<String>,
    },
}

impl SessionActor {
    fn principal_id(&self) -> &str {
        match self {
            Self::Portal { user_id } => user_id,
            Self::Application { token_id, .. } => token_id,
        }
    }

    fn actor_ref(&self) -> String {
        match self {
            Self::Portal { user_id } => format!("user:{user_id}"),
            Self::Application { token_id, .. } => format!("application_token:{token_id}"),
        }
    }

    pub(super) fn require_scope(&self, scope: &str) -> Result<(), StatusCode> {
        match self {
            Self::Portal { .. } => Ok(()),
            Self::Application { scopes, .. } if scopes.iter().any(|value| value == scope) => Ok(()),
            Self::Application { .. } => Err(StatusCode::FORBIDDEN),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionCreateRequest {
    pub(crate) target_id: String,
    pub(crate) device_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SessionDto {
    id: String,
    target_id: String,
    device_id: String,
    status: String,
    started_at: String,
    ended_at: Option<String>,
    created_at: String,
    tenant_id: String,
}

#[derive(Serialize)]
struct SessionPage {
    data: Vec<SessionDto>,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct SessionListQuery {
    pub(crate) target_id: Option<String>,
    pub(crate) device_id: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) limit: Option<u16>,
    pub(crate) cursor: Option<String>,
}

pub(super) async fn request_actor(
    state: &AppState,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<(PgPool, SessionActor, String), StatusCode> {
    let pool = state
        .tracking_pool
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let tenant = headers
        .get("x-organization-id")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)
        .and_then(|value| {
            uuid::Uuid::parse_str(value)
                .map(|value| value.to_string())
                .map_err(|_| StatusCode::BAD_REQUEST)
        })?;

    if let Some(value) = headers.get(header::AUTHORIZATION) {
        let value = value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
        let token = value
            .strip_prefix("Bearer ")
            .filter(|token| token.starts_with("gps_app_"))
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let identity = tracking_storage::lookup_application_token(&pool, &digest(token))
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if identity.tenant_id != tenant {
            return Err(StatusCode::FORBIDDEN);
        }
        return Ok((
            pool,
            SessionActor::Application {
                token_id: identity.token_id,
                scopes: identity.scopes,
            },
            tenant,
        ));
    }

    let auth = state.auth.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = if mutation {
        session_user_csrf(auth, headers).await?
    } else {
        session_user(auth, headers).await?
    };
    let mut auth_tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM memberships m JOIN organizations o ON o.id=m.tenant_id \
         WHERE m.tenant_id=$1::uuid AND m.user_id=$2::uuid AND m.status='active' AND o.status='active')",
    )
    .bind(&tenant)
    .bind(&user.user_id)
    .fetch_one(&mut *auth_tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    auth_tx
        .commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !active {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok((
        pool,
        SessionActor::Portal {
            user_id: user.user_id,
        },
        tenant,
    ))
}

pub(super) fn valid_uuid(value: &str) -> Result<String, StatusCode> {
    uuid::Uuid::parse_str(value)
        .map(|id| id.to_string())
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)
}

pub(super) async fn normalized_time(
    pool: &PgPool,
    value: Option<&str>,
) -> Result<Option<String>, StatusCode> {
    let Some(value) = value else { return Ok(None) };
    let timezone = value.ends_with('Z')
        || value.ends_with('z')
        || value
            .as_bytes()
            .get(value.len().saturating_sub(6))
            .is_some_and(|sign| {
                (*sign == b'+' || *sign == b'-')
                    && value.as_bytes().get(value.len().saturating_sub(3)) == Some(&b':')
            });
    if !value.contains('T') || !timezone {
        return Err(StatusCode::BAD_REQUEST);
    }
    sqlx::query_scalar::<_, String>(
        "SELECT to_char($1::timestamptz AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')",
    )
    .bind(value)
    .fetch_one(pool)
    .await
    .map(Some)
    .map_err(|_| StatusCode::BAD_REQUEST)
}

pub(super) async fn require_resource_permission(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &SessionActor,
    tenant: &str,
    resource_type: &str,
    resource_id: &str,
    permission: &str,
) -> Result<(), StatusCode> {
    let allowed: bool = match actor {
        SessionActor::Portal { user_id } => sqlx::query_scalar(
            "SELECT gps_tracking_member_has_permission($1::uuid,$2::uuid,$3,$4::uuid,$5)",
        )
        .bind(tenant)
        .bind(user_id)
        .bind(resource_type)
        .bind(resource_id)
        .bind(permission)
        .fetch_one(&mut **tx)
        .await,
        SessionActor::Application { token_id, .. } => sqlx::query_scalar(
            "SELECT gps_tracking_application_token_owner_has_session_permission($1::uuid,$2::uuid,$3,$4::uuid,$5)",
        )
        .bind(token_id)
        .bind(tenant)
        .bind(resource_type)
        .bind(resource_id)
        .bind(permission)
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

pub(super) async fn require_session_access(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &SessionActor,
    tenant: &str,
    target_id: &str,
    device_id: &str,
    permission: &str,
) -> Result<(), StatusCode> {
    require_resource_permission(tx, actor, tenant, "target", target_id, permission).await?;
    require_resource_permission(tx, actor, tenant, "device", device_id, permission).await?;
    Ok(())
}

fn session_dto(
    row: (
        String,
        String,
        String,
        String,
        Option<String>,
        String,
        String,
    ),
) -> SessionDto {
    let status = if row.4.is_some() { "ended" } else { "active" };
    SessionDto {
        id: row.0,
        target_id: row.1,
        device_id: row.2,
        status: status.to_owned(),
        started_at: row.3,
        ended_at: row.4,
        created_at: row.5,
        tenant_id: row.6,
    }
}

async fn load_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    id: &str,
) -> Result<Option<SessionDto>, StatusCode> {
    let row = sqlx::query_as::<_, (String, String, String, String, Option<String>, String, String)>(
        "SELECT id::text,target_id::text,device_id::text, \
         to_char(started_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), \
         CASE WHEN ended_at IS NULL THEN NULL ELSE to_char(ended_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') END, \
         to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),tenant_id::text \
         FROM tracking_sessions WHERE tenant_id=$1::uuid AND id=$2::uuid",
    )
    .bind(tenant)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(row.map(session_dto))
}

fn signed_scope(tenant: &str, query: &SessionListQuery) -> String {
    serde_json::to_string(&(
        "tracking-sessions-v1",
        tenant,
        query.target_id.as_deref(),
        query.device_id.as_deref(),
        query.status.as_deref(),
        query.from.as_deref(),
        query.to.as_deref(),
        "created_at_desc_id_desc",
    ))
    .unwrap_or_default()
}

fn encode_session_cursor(key: &[u8; 32], scope: &str, created_at: &str, id: &str) -> String {
    let claims = serde_json::json!({"scope":scope,"created_at":created_at,"id":id});
    let bytes = serde_json::to_vec(&claims).unwrap_or_default();
    let mut mac = CursorMac::new_from_slice(key).expect("HMAC accepts 32-byte key");
    mac.update(&bytes);
    let mut signed = bytes;
    signed.extend_from_slice(&mac.finalize().into_bytes());
    URL_SAFE_NO_PAD.encode(signed)
}

fn decode_session_cursor(
    key: &[u8; 32],
    scope: &str,
    encoded: &str,
) -> Result<PageCursor, StatusCode> {
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
    let cursor: PageCursor = serde_json::from_slice(claims).map_err(|_| StatusCode::BAD_REQUEST)?;
    if cursor.scope != scope || uuid::Uuid::parse_str(&cursor.id).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(cursor)
}

pub(crate) async fn create_tracking_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SessionCreateRequest>,
) -> Result<Response, StatusCode> {
    let (pool, actor, tenant) = request_actor(&state, &headers, true).await?;
    actor.require_scope("sessions:write")?;
    let target_id = valid_uuid(&body.target_id)?;
    let device_id = valid_uuid(&body.device_id)?;
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    if key.is_empty() || key.chars().count() > 128 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let request_hash = Sha256::digest(
        serde_json::to_vec(&SessionCreateRequest {
            target_id: target_id.clone(),
            device_id: device_id.clone(),
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    )
    .to_vec();
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM organizations WHERE id=$1::uuid AND status='active' FOR SHARE",
    )
    .bind(&tenant)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::FORBIDDEN)?;
    let lock_key = serde_json::to_string(&(&tenant, actor.principal_id(), SESSION_ROUTE, key))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(lock_key)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let cached = sqlx::query_as::<_, (Vec<u8>, serde_json::Value, i16)>(
        "SELECT request_hash,response,status_code FROM idempotency_records \
         WHERE tenant_id=$1::uuid AND principal_id=$2::uuid AND route_key=$3 AND idempotency_key=$4 \
         AND expires_at>clock_timestamp()",
    )
    .bind(&tenant)
    .bind(actor.principal_id())
    .bind(SESSION_ROUTE)
    .bind(key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some((cached_hash, response, status_code)) = cached {
        if cached_hash != request_hash {
            return Err(StatusCode::CONFLICT);
        }
        let dto: SessionDto =
            serde_json::from_value(response).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        tracking_storage::lock_tracking_resources(
            &mut tx,
            &tenant,
            std::slice::from_ref(&dto.device_id),
            std::slice::from_ref(&dto.target_id),
            &[],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        require_session_access(
            &mut tx,
            &actor,
            &tenant,
            &dto.target_id,
            &dto.device_id,
            "write",
        )
        .await?;
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(session_response(
            StatusCode::from_u16(status_code as u16)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            &dto,
        ));
    }
    sqlx::query("DELETE FROM idempotency_records WHERE tenant_id=$1::uuid AND principal_id=$2::uuid AND route_key=$3 AND idempotency_key=$4 AND expires_at<=clock_timestamp()")
        .bind(&tenant).bind(actor.principal_id()).bind(SESSION_ROUTE).bind(key)
        .execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    require_session_access(&mut tx, &actor, &tenant, &target_id, &device_id, "write").await?;
    let resources_exist: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM targets WHERE tenant_id=$1::uuid AND id=$2::uuid) \
         AND EXISTS(SELECT 1 FROM devices WHERE tenant_id=$1::uuid AND id=$3::uuid)",
    )
    .bind(&tenant)
    .bind(&target_id)
    .bind(&device_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !resources_exist {
        return Err(StatusCode::NOT_FOUND);
    }
    tracking_storage::lock_tracking_resources(
        &mut tx,
        &tenant,
        std::slice::from_ref(&device_id),
        std::slice::from_ref(&target_id),
        &[],
    )
    .await
    .map_err(|error| {
        if matches!(error, sqlx::Error::RowNotFound) {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    require_session_access(&mut tx, &actor, &tenant, &target_id, &device_id, "write").await?;
    let enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM devices WHERE tenant_id=$1::uuid AND id=$2::uuid")
            .bind(&tenant)
            .bind(&device_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if enabled != Some(true) {
        return Err(StatusCode::CONFLICT);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let row = sqlx::query_as::<_, (String, String, String, String, Option<String>, String, String)>(
        "INSERT INTO tracking_sessions(id,tenant_id,target_id,device_id) \
         VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid) \
         RETURNING id::text,target_id::text,device_id::text, \
         to_char(started_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), \
         NULL::text,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),tenant_id::text",
    )
    .bind(&id)
    .bind(&tenant)
    .bind(&target_id)
    .bind(&device_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| {
        if error.as_database_error().is_some_and(|db| {
            matches!(db.code().as_deref(), Some("23P01" | "23505"))
        }) {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    let dto = session_dto(row);
    let response = serde_json::to_value(&dto).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("WITH creation_time AS (SELECT clock_timestamp() AS created_at) \
        INSERT INTO idempotency_records(tenant_id,principal_id,route_key,idempotency_key,request_hash,response,status_code,created_at,expires_at) \
        SELECT $1::uuid,$2::uuid,$3,$4,$5,$6,201,created_at,created_at+interval '24 hours' FROM creation_time")
        .bind(&tenant).bind(actor.principal_id()).bind(SESSION_ROUTE).bind(key).bind(request_hash).bind(response)
        .execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES($1::uuid,$2,'tracking_session.started','tracking_session',$3::uuid,'success',$4)")
        .bind(&tenant).bind(actor.actor_ref()).bind(&dto.id).bind(uuid::Uuid::new_v4().to_string())
        .execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    publish_event(
        &state,
        &tenant,
        Some(&dto.device_id),
        Some(&dto.target_id),
        Some(&dto.id),
        "session.started",
        &dto.started_at,
        serde_json::json!({"session_id":dto.id,"target_id":dto.target_id,"device_id":dto.device_id}),
    );
    Ok(session_response(StatusCode::CREATED, &dto))
}

pub(crate) async fn list_tracking_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SessionListQuery>,
) -> Result<Response, StatusCode> {
    let (pool, actor, tenant) = request_actor(&state, &headers, false).await?;
    actor.require_scope("sessions:read")?;
    let limit = page_limit(&PageQuery {
        limit: query.limit,
        cursor: query.cursor.clone(),
    })?;
    let target_id = query.target_id.as_deref().map(valid_uuid).transpose()?;
    let device_id = query.device_id.as_deref().map(valid_uuid).transpose()?;
    if query
        .status
        .as_deref()
        .is_some_and(|value| !matches!(value, "active" | "ended"))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let from = normalized_time(&pool, query.from.as_deref()).await?;
    let to = normalized_time(&pool, query.to.as_deref()).await?;
    if from
        .as_ref()
        .zip(to.as_ref())
        .is_some_and(|(from, to)| from >= to)
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let scope = signed_scope(&tenant, &query);
    let key = state
        .auth
        .as_ref()
        .and_then(|auth| auth.mfa_key.as_ref())
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|cursor| decode_session_cursor(key, &scope, cursor))
        .transpose()?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let (Some(target_id), Some(device_id)) = (&target_id, &device_id) {
        require_session_access(&mut tx, &actor, &tenant, target_id, device_id, "read").await?;
    } else if let Some(target_id) = &target_id {
        require_resource_permission(&mut tx, &actor, &tenant, "target", target_id, "read").await?;
    } else if let Some(device_id) = &device_id {
        require_resource_permission(&mut tx, &actor, &tenant, "device", device_id, "read").await?;
    }
    let rows = sqlx::query_as::<_, (String, String, String, String, Option<String>, String, String)>(
        "SELECT s.id::text,s.target_id::text,s.device_id::text, \
         to_char(s.started_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), \
         CASE WHEN s.ended_at IS NULL THEN NULL ELSE to_char(s.ended_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') END, \
         to_char(s.created_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'),s.tenant_id::text \
         FROM tracking_sessions s WHERE s.tenant_id=$1::uuid \
         AND ($2::uuid IS NULL OR s.target_id=$2::uuid) AND ($3::uuid IS NULL OR s.device_id=$3::uuid) \
         AND ($4::text IS NULL OR ($4='active' AND s.ended_at IS NULL) OR ($4='ended' AND s.ended_at IS NOT NULL)) \
         AND ($5::timestamptz IS NULL OR s.started_at >= $5::timestamptz) \
         AND ($6::timestamptz IS NULL OR s.started_at < $6::timestamptz) \
         AND ($7::timestamptz IS NULL OR (s.created_at,s.id)<($7::timestamptz,$8::uuid)) \
         AND CASE WHEN $9='portal' THEN \
           gps_tracking_member_has_permission($1::uuid,$10::uuid,'target'::text,s.target_id,'read'::text) AND \
           gps_tracking_member_has_permission($1::uuid,$10::uuid,'device'::text,s.device_id,'read'::text) \
         ELSE gps_tracking_application_token_owner_has_session_permission($11::uuid,$1::uuid,'target'::text,s.target_id,'read'::text) AND \
           gps_tracking_application_token_owner_has_session_permission($11::uuid,$1::uuid,'device'::text,s.device_id,'read'::text) END \
         ORDER BY s.created_at DESC,s.id DESC LIMIT $12",
    )
    .bind(&tenant)
    .bind(&target_id)
    .bind(&device_id)
    .bind(&query.status)
    .bind(&from)
    .bind(&to)
    .bind(cursor.as_ref().map(|cursor| cursor.created_at.as_str()))
    .bind(cursor.as_ref().map(|cursor| cursor.id.as_str()))
    .bind(if matches!(actor, SessionActor::Portal { .. }) { "portal" } else { "application" })
    .bind(match &actor { SessionActor::Portal { user_id } => Some(user_id.as_str()), _ => None })
    .bind(match &actor { SessionActor::Application { token_id, .. } => Some(token_id.as_str()), _ => None })
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
        Some(encode_session_cursor(key, &scope, &row.5, &row.0))
    } else {
        None
    };
    Ok(json_response(
        StatusCode::OK,
        &uuid::Uuid::new_v4().to_string(),
        SessionPage {
            data: rows.into_iter().map(session_dto).collect(),
            next_cursor,
        },
    ))
}

pub(crate) async fn get_tracking_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let (pool, actor, tenant) = request_actor(&state, &headers, false).await?;
    actor.require_scope("sessions:read")?;
    let id = valid_uuid(&id)?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let dto = load_session(&mut tx, &tenant, &id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    require_session_access(
        &mut tx,
        &actor,
        &tenant,
        &dto.target_id,
        &dto.device_id,
        "read",
    )
    .await?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(json_response(
        StatusCode::OK,
        &uuid::Uuid::new_v4().to_string(),
        dto,
    ))
}

pub(crate) async fn end_tracking_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<EmptySessionRequest>,
) -> Result<Response, StatusCode> {
    let _ = body;
    let (pool, actor, tenant) = request_actor(&state, &headers, true).await?;
    actor.require_scope("sessions:write")?;
    let id = valid_uuid(&id)?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let locator = sqlx::query_as::<_, (String, String)>(
        "SELECT target_id::text,device_id::text FROM tracking_sessions WHERE tenant_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    tracking_storage::lock_tracking_resources(
        &mut tx,
        &tenant,
        std::slice::from_ref(&locator.1),
        std::slice::from_ref(&locator.0),
        std::slice::from_ref(&id),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let dto = load_session(&mut tx, &tenant, &id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    require_session_access(
        &mut tx,
        &actor,
        &tenant,
        &dto.target_id,
        &dto.device_id,
        "write",
    )
    .await?;
    let was_active = dto.ended_at.is_none();
    if was_active {
        sqlx::query("UPDATE tracking_sessions SET ended_at=GREATEST(clock_timestamp(),started_at) WHERE tenant_id=$1::uuid AND id=$2::uuid AND ended_at IS NULL")
            .bind(&tenant).bind(&id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let dto = load_session(&mut tx, &tenant, &id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    if was_active {
        sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES($1::uuid,$2,'tracking_session.ended','tracking_session',$3::uuid,'success',$4)")
            .bind(&tenant).bind(actor.actor_ref()).bind(&id).bind(uuid::Uuid::new_v4().to_string())
            .execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if was_active {
        let ended_at = dto.ended_at.as_deref().unwrap_or(&dto.started_at);
        publish_event(
            &state,
            &tenant,
            Some(&dto.device_id),
            Some(&dto.target_id),
            Some(&dto.id),
            "session.ended",
            ended_at,
            serde_json::json!({"session_id":dto.id,"target_id":dto.target_id,"device_id":dto.device_id,"ended_at":ended_at}),
        );
    }
    Ok(json_response(
        StatusCode::OK,
        &uuid::Uuid::new_v4().to_string(),
        dto,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptySessionRequest {}

fn session_response(status: StatusCode, dto: &SessionDto) -> Response {
    let mut response = json_response(status, &uuid::Uuid::new_v4().to_string(), dto);
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&format!("/v1/sessions/{}", dto.id))
            .unwrap_or_else(|_| HeaderValue::from_static("/v1/sessions")),
    );
    response
}

fn json_response<T: Serialize>(status: StatusCode, request_id: &str, value: T) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}
