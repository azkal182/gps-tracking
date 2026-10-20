use super::*;

#[derive(Serialize)]
pub(super) struct TargetDto {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) external_ref: Option<String>,
    pub(super) metadata: serde_json::Value,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) tenant_id: String,
}

#[derive(Serialize)]
pub(super) struct DeviceDto {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) protocol: String,
    pub(super) identifier: String,
    pub(super) ingestion_policy: String,
    pub(super) metadata: serde_json::Value,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) tenant_id: String,
    pub(super) enabled: bool,
}

#[derive(Serialize)]
pub(super) struct DeviceCredentialDto {
    pub(super) id: String,
    pub(super) created_at: String,
    pub(super) revoked_at: Option<String>,
    pub(super) active: bool,
}

#[derive(Serialize)]
pub(super) struct DeviceCredentialRotationDto {
    pub(super) device_id: String,
    pub(super) token: String,
    pub(super) created_at: String,
}

#[derive(Serialize)]
pub(super) struct ResourcePage<T> {
    pub(super) data: Vec<T>,
    pub(super) next_cursor: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceQuery {
    pub(super) limit: Option<u16>,
    pub(super) cursor: Option<String>,
    pub(super) q: Option<String>,
    pub(super) protocol: Option<String>,
}

fn resource_tenant(headers: &HeaderMap) -> Result<String, StatusCode> {
    headers
        .get("x-organization-id")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)
        .and_then(|value| {
            uuid::Uuid::parse_str(value)
                .map(|id| id.to_string())
                .map_err(|_| StatusCode::BAD_REQUEST)
        })
}

fn validate_resource_id(id: &str) -> Result<(), StatusCode> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| StatusCode::BAD_REQUEST)
}

fn resource_query(query: &ResourceQuery) -> Result<(i64, Option<String>), StatusCode> {
    let limit = page_limit(&PageQuery {
        limit: query.limit,
        cursor: None,
    })?;
    let q = query.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    if q.is_some_and(|q| q.len() > 150) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok((limit, q.map(str::to_owned)))
}

fn target_dto(
    row: (
        String,
        String,
        Option<String>,
        serde_json::Value,
        String,
        String,
        String,
    ),
) -> TargetDto {
    TargetDto {
        id: row.0,
        name: row.1,
        external_ref: row.2,
        metadata: row.3,
        created_at: row.4,
        updated_at: row.5,
        tenant_id: row.6,
    }
}

fn device_dto(
    row: (
        String,
        String,
        String,
        String,
        String,
        serde_json::Value,
        String,
        String,
        String,
        bool,
    ),
) -> DeviceDto {
    DeviceDto {
        id: row.0,
        name: row.1,
        protocol: row.2,
        identifier: row.3,
        ingestion_policy: row.4,
        metadata: row.5,
        created_at: row.6,
        updated_at: row.7,
        tenant_id: row.8,
        enabled: row.9,
    }
}

fn validate_resource_name(value: &serde_json::Value) -> Result<String, StatusCode> {
    let name = value
        .as_str()
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?
        .trim();
    if !(1..=150).contains(&name.chars().count()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(name.to_owned())
}

fn validate_metadata(value: Option<&serde_json::Value>) -> Result<serde_json::Value, StatusCode> {
    let value = value.cloned().unwrap_or_else(|| serde_json::json!({}));
    if !value.is_object() || value.to_string().len() > 16_384 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(value)
}

fn validate_patch_keys<'a>(
    body: &'a serde_json::Value,
    allowed: &[&str],
) -> Result<&'a serde_json::Map<String, serde_json::Value>, StatusCode> {
    let object = body.as_object().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    if object.is_empty() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(object)
}

async fn resource_request_user(
    state: &AppState,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<(AuthState, AuthenticatedUser, String, PgPool), StatusCode> {
    let auth = state.auth.clone().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = if mutation {
        session_user_csrf(&auth, headers).await?
    } else {
        session_user(&auth, headers).await?
    };
    let tenant = resource_tenant(headers)?;
    let pool = state
        .tracking_pool
        .clone()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    Ok((auth, user, tenant, pool))
}

fn cursor_scope(resource: &str, tenant: &str, q: Option<&str>, protocol: Option<&str>) -> String {
    format!(
        "{resource}:{tenant}:{}:{}:created_at_desc_id_desc",
        q.unwrap_or(""),
        protocol.unwrap_or("")
    )
}

pub(super) async fn create_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<
    (
        StatusCode,
        [(header::HeaderName, String); 1],
        Json<TargetDto>,
    ),
    StatusCode,
> {
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, true).await?;
    let object = body.as_object().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    if object
        .keys()
        .any(|key| !["name", "external_ref", "metadata"].contains(&key.as_str()))
        || object.contains_key("id")
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    require_object_permission(
        &auth.pool,
        &tenant,
        &user.user_id,
        "organization",
        &tenant,
        "targets:create",
    )
    .await?;
    let name = validate_resource_name(object.get("name").ok_or(StatusCode::UNPROCESSABLE_ENTITY)?)?;
    let external_ref = match object.get("external_ref") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => {
            let value = value
                .as_str()
                .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?
                .trim();
            if !(1..=128).contains(&value.chars().count()) {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            Some(value.to_owned())
        }
    };
    let metadata = validate_metadata(object.get("metadata"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = sqlx::query_as::<_, (String,String,Option<String>,serde_json::Value,String,String,String)>(
        "INSERT INTO targets(id,tenant_id,name,external_ref,metadata) VALUES($1::uuid,$2::uuid,$3,$4,$5) RETURNING id::text,name,external_ref,metadata,created_at::text,updated_at::text,tenant_id::text")
        .bind(&id).bind(&tenant).bind(name).bind(external_ref).bind(metadata).fetch_one(&mut *tx).await.map_err(map_resource_error)?;
    sqlx::query(
        "SELECT gps_tracking_grant_creator_object_access($1::uuid,$2::uuid,'target',$3::uuid)",
    )
    .bind(&tenant)
    .bind(&user.user_id)
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let location = format!("/v1/targets/{id}");
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, location)],
        Json(target_dto(row)),
    ))
}

pub(super) async fn list_targets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ResourceQuery>,
) -> Result<Json<ResourcePage<TargetDto>>, StatusCode> {
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, false).await?;
    let (limit, q) = resource_query(&query)?;
    let scope = cursor_scope("targets", &tenant, q.as_deref(), None);
    let key = auth
        .mfa_key
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|v| decode_page_cursor(key, &scope, v))
        .transpose()?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows=sqlx::query_as::<_,(String,String,Option<String>,serde_json::Value,String,String,String)>("SELECT id::text,name,external_ref,metadata,created_at::text,updated_at::text,tenant_id::text FROM targets WHERE tenant_id=$1::uuid AND gps_tracking_member_has_permission($1::uuid,$2::uuid,'target',id,'read') AND ($3::text IS NULL OR name ILIKE '%' || $3 || '%') AND ($4::timestamptz IS NULL OR (created_at,id)<($4::timestamptz,$5::uuid)) ORDER BY created_at DESC,id DESC LIMIT $6")
       .bind(&tenant).bind(&user.user_id).bind(&q).bind(cursor.as_ref().map(|c|c.created_at.as_str())).bind(cursor.as_ref().map(|c|c.id.as_str())).bind(limit+1).fetch_all(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut rows = rows;
    let more = rows.len() as i64 > limit;
    if more {
        rows.truncate(limit as usize)
    }
    let next_cursor = if more {
        let row = rows.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(
            encode_page_cursor(key, &scope, &row.5, &row.0)
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    Ok(Json(ResourcePage {
        data: rows.into_iter().map(target_dto).collect(),
        next_cursor,
    }))
}

async fn target_by_id(
    pool: &PgPool,
    tenant: &str,
    id: &str,
) -> Result<Option<TargetDto>, StatusCode> {
    let mut tx = tracking_storage::begin_tenant_transaction(pool, tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row=sqlx::query_as::<_,(String,String,Option<String>,serde_json::Value,String,String,String)>("SELECT id::text,name,external_ref,metadata,created_at::text,updated_at::text,tenant_id::text FROM targets WHERE tenant_id=$1::uuid AND id=$2::uuid").bind(tenant).bind(id).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(row.map(target_dto))
}

pub(super) async fn get_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<TargetDto>, StatusCode> {
    validate_resource_id(&id)?;
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, false).await?;
    let dto = target_by_id(&pool, &tenant, &id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    require_object_permission(&auth.pool, &tenant, &user.user_id, "target", &id, "read").await?;
    Ok(Json(dto))
}

pub(super) async fn update_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<TargetDto>, StatusCode> {
    validate_resource_id(&id)?;
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, true).await?;
    let fields = validate_patch_keys(&body, &["name", "external_ref", "metadata"])?;
    require_object_permission(&auth.pool, &tenant, &user.user_id, "target", &id, "write").await?;
    let name = fields.get("name").map(validate_resource_name).transpose()?;
    let external = if let Some(value) = fields.get("external_ref") {
        if value.is_null() {
            Some(None)
        } else {
            let v = value
                .as_str()
                .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?
                .trim();
            if !(1..=128).contains(&v.chars().count()) {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            Some(Some(v.to_owned()))
        }
    } else {
        None
    };
    let metadata = fields
        .get("metadata")
        .map(|v| validate_metadata(Some(v)))
        .transpose()?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row=sqlx::query_as::<_,(String,String,Option<String>,serde_json::Value,String,String,String)>("UPDATE targets SET name=COALESCE($3,name),external_ref=CASE WHEN $4 THEN $5 ELSE external_ref END,metadata=COALESCE($6,metadata),updated_at=clock_timestamp() WHERE tenant_id=$1::uuid AND id=$2::uuid RETURNING id::text,name,external_ref,metadata,created_at::text,updated_at::text,tenant_id::text")
      .bind(&tenant).bind(&id).bind(name).bind(external.is_some()).bind(external.flatten()).bind(metadata).fetch_optional(&mut *tx).await.map_err(map_resource_error)?.ok_or(StatusCode::NOT_FOUND)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(target_dto(row)))
}

pub(super) async fn create_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<
    (
        StatusCode,
        [(header::HeaderName, String); 1],
        Json<DeviceDto>,
    ),
    StatusCode,
> {
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, true).await?;
    let object = body.as_object().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    if object.keys().any(|key| {
        ![
            "name",
            "protocol",
            "identifier",
            "ingestion_policy",
            "metadata",
        ]
        .contains(&key.as_str())
    }) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    require_object_permission(
        &auth.pool,
        &tenant,
        &user.user_id,
        "organization",
        &tenant,
        "devices:create",
    )
    .await?;
    let name = validate_resource_name(object.get("name").ok_or(StatusCode::UNPROCESSABLE_ENTITY)?)?;
    let protocol = object
        .get("protocol")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    if protocol != "http-json" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let identifier = object
        .get("identifier")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?
        .trim();
    if !(1..=128).contains(&identifier.chars().count()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let policy = object
        .get("ingestion_policy")
        .and_then(|v| v.as_str())
        .unwrap_or("session_only");
    if !["session_only", "always"].contains(&policy) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let metadata = validate_metadata(object.get("metadata"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row=sqlx::query_as::<_,(String,String,String,String,String,serde_json::Value,String,String,String,bool)>("INSERT INTO devices(id,tenant_id,name,protocol,identifier,ingestion_policy,metadata) VALUES($1::uuid,$2::uuid,$3,$4,$5,$6,$7) RETURNING id::text,name,protocol,identifier,ingestion_policy,metadata,created_at::text,updated_at::text,tenant_id::text,enabled")
      .bind(&id).bind(&tenant).bind(name).bind(protocol).bind(identifier).bind(policy).bind(metadata).fetch_one(&mut *tx).await.map_err(map_resource_error)?;
    sqlx::query(
        "SELECT gps_tracking_grant_creator_object_access($1::uuid,$2::uuid,'device',$3::uuid)",
    )
    .bind(&tenant)
    .bind(&user.user_id)
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::CREATED,
        [(header::LOCATION, format!("/v1/devices/{id}"))],
        Json(device_dto(row)),
    ))
}

pub(super) async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ResourceQuery>,
) -> Result<Json<ResourcePage<DeviceDto>>, StatusCode> {
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, false).await?;
    let (limit, q) = resource_query(&query)?;
    if query.protocol.as_deref().is_some_and(|p| p != "http-json") {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let scope = cursor_scope("devices", &tenant, q.as_deref(), query.protocol.as_deref());
    let key = auth
        .mfa_key
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let cursor = query
        .cursor
        .as_deref()
        .map(|v| decode_page_cursor(key, &scope, v))
        .transpose()?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows=sqlx::query_as::<_,(String,String,String,String,String,serde_json::Value,String,String,String,bool)>("SELECT id::text,name,protocol,identifier,ingestion_policy,metadata,created_at::text,updated_at::text,tenant_id::text,enabled FROM devices WHERE tenant_id=$1::uuid AND gps_tracking_member_has_permission($1::uuid,$2::uuid,'device',id,'read') AND ($3::text IS NULL OR name ILIKE '%'||$3||'%') AND ($4::text IS NULL OR protocol=$4) AND ($5::timestamptz IS NULL OR (created_at,id)<($5::timestamptz,$6::uuid)) ORDER BY created_at DESC,id DESC LIMIT $7")
     .bind(&tenant).bind(&user.user_id).bind(&q).bind(&query.protocol).bind(cursor.as_ref().map(|c|c.created_at.as_str())).bind(cursor.as_ref().map(|c|c.id.as_str())).bind(limit+1).fetch_all(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut rows = rows;
    let more = rows.len() as i64 > limit;
    if more {
        rows.truncate(limit as usize)
    }
    let next_cursor = if more {
        let row = rows.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(
            encode_page_cursor(key, &scope, &row.6, &row.0)
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    Ok(Json(ResourcePage {
        data: rows.into_iter().map(device_dto).collect(),
        next_cursor,
    }))
}

async fn device_by_id(
    pool: &PgPool,
    tenant: &str,
    id: &str,
) -> Result<Option<DeviceDto>, StatusCode> {
    let mut tx = tracking_storage::begin_tenant_transaction(pool, tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row=sqlx::query_as::<_,(String,String,String,String,String,serde_json::Value,String,String,String,bool)>("SELECT id::text,name,protocol,identifier,ingestion_policy,metadata,created_at::text,updated_at::text,tenant_id::text,enabled FROM devices WHERE tenant_id=$1::uuid AND id=$2::uuid").bind(tenant).bind(id).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(row.map(device_dto))
}
pub(super) async fn get_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<DeviceDto>, StatusCode> {
    validate_resource_id(&id)?;
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, false).await?;
    let dto = device_by_id(&pool, &tenant, &id)
        .await?
        .ok_or(StatusCode::NOT_FOUND)?;
    require_object_permission(&auth.pool, &tenant, &user.user_id, "device", &id, "read").await?;
    Ok(Json(dto))
}
pub(super) async fn update_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<DeviceDto>, StatusCode> {
    validate_resource_id(&id)?;
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, true).await?;
    let fields = validate_patch_keys(&body, &["name", "metadata", "ingestion_policy", "enabled"])?;
    require_object_permission(&auth.pool, &tenant, &user.user_id, "device", &id, "write").await?;
    let name = fields.get("name").map(validate_resource_name).transpose()?;
    let metadata = fields
        .get("metadata")
        .map(|v| validate_metadata(Some(v)))
        .transpose()?;
    let policy = fields
        .get("ingestion_policy")
        .map(|v| {
            v.as_str()
                .filter(|v| ["session_only", "always"].contains(v))
                .ok_or(StatusCode::UNPROCESSABLE_ENTITY)
        })
        .transpose()?;
    if policy == Some("always") {
        require_tenant_admin(&auth, &user, &headers).await?;
    }
    let enabled = fields
        .get("enabled")
        .map(|v| v.as_bool().ok_or(StatusCode::UNPROCESSABLE_ENTITY))
        .transpose()?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if enabled.is_some() {
        lock_and_authorize_device(&mut tx, &tenant, &user.user_id, &id, "write").await?;
    }
    let row=sqlx::query_as::<_,(String,String,String,String,String,serde_json::Value,String,String,String,bool)>("UPDATE devices SET name=COALESCE($3,name),metadata=COALESCE($4,metadata),ingestion_policy=COALESCE($5,ingestion_policy),enabled=COALESCE($6,enabled),updated_at=clock_timestamp() WHERE tenant_id=$1::uuid AND id=$2::uuid RETURNING id::text,name,protocol,identifier,ingestion_policy,metadata,created_at::text,updated_at::text,tenant_id::text,enabled")
     .bind(&tenant).bind(&id).bind(name).bind(metadata).bind(policy).bind(enabled).fetch_optional(&mut *tx).await.map_err(map_resource_error)?.ok_or(StatusCode::NOT_FOUND)?;
    if enabled == Some(false) {
        sqlx::query("UPDATE device_credentials SET revoked_at=COALESCE(revoked_at,clock_timestamp()) WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND revoked_at IS NULL")
            .bind(&tenant).bind(&id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(device_dto(row)))
}

fn map_resource_error(error: sqlx::Error) -> StatusCode {
    match error.as_database_error().and_then(|e| e.code()).as_deref() {
        Some("23505") => StatusCode::CONFLICT,
        Some("23514" | "22001" | "22P02") => StatusCode::UNPROCESSABLE_ENTITY,
        Some("42501") => StatusCode::FORBIDDEN,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn lock_and_authorize_device(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    user_id: &str,
    device_id: &str,
    permission: &str,
) -> Result<bool, StatusCode> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM devices WHERE tenant_id=$1::uuid AND id=$2::uuid)",
    )
    .bind(tenant)
    .bind(device_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    tracking_storage::lock_tracking_resources(tx, tenant, &[device_id.to_owned()], &[], &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status =
        sqlx::query_scalar::<_, String>("SELECT status FROM organizations WHERE id=$1::uuid")
            .bind(tenant)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if status.as_deref() != Some("active") {
        return Err(StatusCode::FORBIDDEN);
    }
    let enabled = sqlx::query_scalar::<_, bool>(
        "SELECT enabled FROM devices WHERE tenant_id=$1::uuid AND id=$2::uuid",
    )
    .bind(tenant)
    .bind(device_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let allowed: bool = sqlx::query_scalar(
        "SELECT gps_tracking_member_has_permission($1::uuid,$2::uuid,'device',$3::uuid,$4)",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(device_id)
    .bind(permission)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(enabled)
}

pub(super) async fn rotate_device_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(_body): Json<EmptyRequest>,
) -> Result<Response, StatusCode> {
    validate_resource_id(&id)?;
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, true).await?;
    require_recent_auth(&auth, &user, true).await?;
    require_object_permission(
        &auth.pool,
        &tenant,
        &user.user_id,
        "device",
        &id,
        "credentials:write",
    )
    .await?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let enabled =
        lock_and_authorize_device(&mut tx, &tenant, &user.user_id, &id, "credentials:write")
            .await?;
    if !enabled {
        return Err(StatusCode::CONFLICT);
    }
    let protocol = sqlx::query_scalar::<_, String>(
        "SELECT protocol FROM devices WHERE tenant_id=$1::uuid AND id=$2::uuid",
    )
    .bind(&tenant)
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if protocol != "http-json" {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let raw_token = format!("gps_live_{}", random_secret());
    sqlx::query("UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND revoked_at IS NULL")
        .bind(&tenant).bind(&id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let created = sqlx::query_scalar::<_, String>(
        "INSERT INTO device_credentials(id,tenant_id,device_id,token_hash) VALUES($1::uuid,$2::uuid,$3::uuid,$4) RETURNING created_at::text",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&tenant)
    .bind(&id)
    .bind(digest(&raw_token))
    .fetch_one(&mut *tx)
    .await
    .map_err(map_resource_error)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(DeviceCredentialRotationDto {
            device_id: id,
            token: raw_token,
            created_at: created,
        }),
    )
        .into_response())
}

pub(super) async fn list_device_credentials(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Response, StatusCode> {
    validate_resource_id(&id)?;
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, false).await?;
    require_object_permission(
        &auth.pool,
        &tenant,
        &user.user_id,
        "device",
        &id,
        "credentials:write",
    )
    .await?;
    let limit = page_limit(&query)?;
    let scope = format!("device-credentials:{tenant}:{id}:created_at_desc_id_desc");
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
    let device_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM devices WHERE tenant_id=$1::uuid AND id=$2::uuid)",
    )
    .bind(&tenant)
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !device_exists {
        return Err(StatusCode::NOT_FOUND);
    }
    let rows = sqlx::query_as::<_, (String, String, Option<String>, bool)>(
        "SELECT id::text,created_at::text,revoked_at::text,revoked_at IS NULL FROM device_credentials WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND ($3::timestamptz IS NULL OR (created_at,id)<($3::timestamptz,$4::uuid)) ORDER BY created_at DESC,id DESC LIMIT $5",
    )
    .bind(&tenant)
    .bind(&id)
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
            encode_page_cursor(key, &scope, &row.1, &row.0)
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    let data = rows
        .into_iter()
        .map(|row| DeviceCredentialDto {
            id: row.0,
            created_at: row.1,
            revoked_at: row.2,
            active: row.3,
        })
        .collect::<Vec<_>>();
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(ResourcePage { data, next_cursor }),
    )
        .into_response())
}

pub(super) async fn revoke_device_credential(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path((id, credential_id)): axum::extract::Path<(String, String)>,
) -> Result<Response, StatusCode> {
    validate_resource_id(&id)?;
    validate_resource_id(&credential_id)?;
    let (auth, user, tenant, pool) = resource_request_user(&state, &headers, true).await?;
    require_recent_auth(&auth, &user, true).await?;
    require_object_permission(
        &auth.pool,
        &tenant,
        &user.user_id,
        "device",
        &id,
        "credentials:write",
    )
    .await?;
    let mut tx = tracking_storage::begin_tenant_transaction(&pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    lock_and_authorize_device(&mut tx, &tenant, &user.user_id, &id, "credentials:write").await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM device_credentials WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND id=$3::uuid)",
    )
    .bind(&tenant)
    .bind(&id)
    .bind(&credential_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !exists {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query("UPDATE device_credentials SET revoked_at=COALESCE(revoked_at,clock_timestamp()) WHERE tenant_id=$1::uuid AND device_id=$2::uuid AND id=$3::uuid")
        .bind(&tenant).bind(&id).bind(&credential_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}
