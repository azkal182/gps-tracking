use super::*;

#[derive(Serialize)]
pub(super) struct OrganizationDto {
    pub(super) id: String,
    pub(super) tenant_id: String,
    pub(super) name: String,
    pub(super) status: String,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Serialize)]
pub(super) struct InvitationDto {
    pub(super) id: String,
    pub(super) tenant_id: String,
    pub(super) email: String,
    pub(super) role: String,
    pub(super) status: String,
    pub(super) expires_at: String,
    pub(super) created_at: String,
}

#[derive(Serialize)]
pub(super) struct OrganizationPage {
    pub(super) data: Vec<OrganizationDto>,
    pub(super) next_cursor: Option<String>,
}

#[derive(Serialize)]
pub(super) struct InvitationPage {
    pub(super) data: Vec<InvitationDto>,
    pub(super) next_cursor: Option<String>,
}

#[derive(Serialize)]
pub(super) struct MembershipUserDto {
    pub(super) email: String,
    pub(super) display_name: Option<String>,
}

#[derive(Serialize)]
pub(super) struct MembershipDto {
    pub(super) id: String,
    pub(super) tenant_id: String,
    pub(super) user_id: String,
    pub(super) role: String,
    pub(super) status: String,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) user: MembershipUserDto,
}

#[derive(Serialize)]
pub(super) struct MembershipPage {
    pub(super) data: Vec<MembershipDto>,
    pub(super) next_cursor: Option<String>,
}

#[derive(Serialize, Clone, PartialEq, Eq)]
pub(super) struct GrantDto {
    pub(super) resource_type: String,
    pub(super) resource_id: String,
    pub(super) permissions: Vec<String>,
}

#[derive(Serialize)]
pub(super) struct GrantList {
    pub(super) grants: Vec<GrantDto>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MembershipPatchRequest {
    pub(super) role: Option<String>,
    pub(super) status: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GrantReplaceRequest {
    pub(super) grants: Vec<GrantInput>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GrantInput {
    pub(super) resource_type: String,
    pub(super) resource_id: String,
    pub(super) permissions: Vec<String>,
}

#[derive(Serialize)]
pub(super) struct CreatedOrganization {
    pub(super) organization: OrganizationDto,
    pub(super) invitation_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OrganizationCreateRequest {
    pub(super) name: String,
    pub(super) admin_email: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OrganizationNameRequest {
    pub(super) name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OrganizationStatusRequest {
    pub(super) status: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InvitationCreateRequest {
    pub(super) email: String,
    pub(super) role: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InvitationAcceptRequest {
    pub(super) token: String,
    pub(super) password: Option<String>,
    pub(super) display_name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct PageQuery {
    pub(super) limit: Option<u16>,
    pub(super) cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PageCursor {
    pub(super) scope: String,
    pub(super) created_at: String,
    pub(super) id: String,
}

pub(super) type CursorMac = Hmac<Sha256>;

pub(super) fn page_limit(query: &PageQuery) -> Result<i64, StatusCode> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(i64::from(limit))
}

pub(super) fn encode_page_cursor(
    key: &[u8; 32],
    scope: &str,
    created_at: &str,
    id: &str,
) -> Option<String> {
    let claims = serde_json::to_vec(&PageCursor {
        scope: scope.to_owned(),
        created_at: created_at.to_owned(),
        id: id.to_owned(),
    })
    .ok()?;
    let mut mac = CursorMac::new_from_slice(key).ok()?;
    mac.update(&claims);
    let mut signed = claims;
    signed.extend_from_slice(&mac.finalize().into_bytes());
    Some(URL_SAFE_NO_PAD.encode(signed))
}

pub(super) fn decode_page_cursor(
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
    let split = bytes.len() - 32;
    let (claims, signature) = bytes.split_at(split);
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InvitationReissueRequest {
    pub(super) email: String,
}

pub(super) fn canonical_email(value: &str) -> Result<String, StatusCode> {
    let value = value.trim().to_lowercase();
    if value.len() > 320
        || value.len() < 3
        || value.contains(char::is_whitespace)
        || value.matches('@').count() != 1
        || value.starts_with('@')
        || value.ends_with('@')
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mailbox = value
        .parse::<lettre::message::Mailbox>()
        .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    if mailbox.email.to_string() != value {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(value)
}

pub(super) fn valid_name(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= 150
}

pub(super) fn organization_dto(row: (String, String, String, String, String)) -> OrganizationDto {
    OrganizationDto {
        id: row.0.clone(),
        tenant_id: row.0,
        name: row.1,
        status: row.2,
        created_at: row.3,
        updated_at: row.4,
    }
}

pub(super) fn invitation_dto(
    row: (String, String, String, String, String, String, String),
) -> InvitationDto {
    InvitationDto {
        id: row.0,
        tenant_id: row.1,
        email: row.2,
        role: if row.3 == "organization_admin" {
            "admin".into()
        } else {
            row.3
        },
        status: row.4,
        expires_at: row.5,
        created_at: row.6,
    }
}

pub(super) fn database_role(role: &str) -> Result<&'static str, StatusCode> {
    match role {
        "admin" | "organization_admin" => Ok("organization_admin"),
        "operator" => Ok("operator"),
        "viewer" => Ok("viewer"),
        _ => Err(StatusCode::UNPROCESSABLE_ENTITY),
    }
}

pub(super) async fn require_platform_admin(
    auth: &AuthState,
    user: &AuthenticatedUser,
) -> Result<(), StatusCode> {
    require_platform_account(auth, user).await?;
    require_recent_auth(auth, user, true).await
}

pub(super) async fn require_platform_account(
    auth: &AuthState,
    user: &AuthenticatedUser,
) -> Result<(), StatusCode> {
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users u WHERE u.id=$1::uuid AND u.status='active' AND u.platform_admin)")
        .bind(&user.user_id).fetch_one(&auth.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub(super) async fn require_tenant_admin(
    auth: &AuthState,
    user: &AuthenticatedUser,
    headers: &HeaderMap,
) -> Result<String, StatusCode> {
    let tenant = headers
        .get("x-organization-id")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let tenant = uuid::Uuid::parse_str(tenant)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_string();
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let role = sqlx::query_scalar::<_, String>("SELECT m.role FROM memberships m JOIN organizations o ON o.id=m.tenant_id WHERE m.tenant_id=$1::uuid AND m.user_id=$2::uuid AND m.status='active' AND o.status='active'")
        .bind(&tenant).bind(&user.user_id).fetch_optional(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if role.as_deref() != Some("organization_admin") {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(tenant)
}

pub(super) async fn require_tenant_member(
    auth: &AuthState,
    user: &AuthenticatedUser,
    headers: &HeaderMap,
) -> Result<String, StatusCode> {
    let tenant = headers
        .get("x-organization-id")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let tenant = uuid::Uuid::parse_str(tenant)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .to_string();
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM memberships m JOIN organizations o ON o.id=m.tenant_id WHERE m.tenant_id=$1::uuid AND m.user_id=$2::uuid AND m.status='active' AND o.status='active')")
        .bind(&tenant).bind(&user.user_id).fetch_one(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if active {
        Ok(tenant)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub(super) async fn require_recent_auth(
    auth: &AuthState,
    user: &AuthenticatedUser,
    require_mfa: bool,
) -> Result<(), StatusCode> {
    let recent: bool = if require_mfa {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_sessions s JOIN users u ON u.id=s.user_id WHERE s.id=$1::uuid AND s.reauthenticated_at>clock_timestamp()-interval '5 minutes' AND u.status='active' AND EXISTS(SELECT 1 FROM mfa_factors f WHERE f.user_id=u.id AND f.enabled_at IS NOT NULL))")
    } else {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_sessions WHERE id=$1::uuid AND reauthenticated_at>clock_timestamp()-interval '5 minutes')")
    }.bind(&user.session_id).fetch_one(&auth.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if recent {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

pub(super) async fn list_my_organizations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Result<Json<OrganizationPage>, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user(&auth, &headers).await?;
    let limit = page_limit(&query)?;
    let scope = format!("organizations:user:{}", user.user_id);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| {
            decode_page_cursor(
                auth.mfa_key
                    .as_ref()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
                &scope,
                value,
            )
        })
        .transpose()?;
    let rows = sqlx::query_as::<_, (String,String,String,String,String)>("SELECT id::text,name,status,created_at::text,updated_at::text FROM gps_tracking_list_user_organizations($1::uuid) WHERE $2::timestamptz IS NULL OR (created_at,id)<($2::timestamptz,$3::uuid) ORDER BY created_at DESC,id DESC LIMIT $4")
        .bind(&user.user_id).bind(cursor.as_ref().map(|c|c.created_at.as_str())).bind(cursor.as_ref().map(|c|c.id.as_str())).bind(limit + 1).fetch_all(&auth.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let has_more = rows.len() as i64 > limit;
    let mut rows = rows;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let last = rows.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(
            encode_page_cursor(
                auth.mfa_key
                    .as_ref()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
                &scope,
                &last.3,
                &last.0,
            )
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    Ok(Json(OrganizationPage {
        data: rows.into_iter().map(organization_dto).collect(),
        next_cursor,
    }))
}

pub(super) async fn list_platform_organizations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Result<Json<OrganizationPage>, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user(&auth, &headers).await?;
    require_platform_account(&auth, &user).await?;
    let pool = auth
        .platform_pool
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let limit = page_limit(&query)?;
    let scope = format!("platform:organizations:{}", user.user_id);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| {
            decode_page_cursor(
                auth.mfa_key
                    .as_ref()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
                &scope,
                value,
            )
        })
        .transpose()?;
    let rows = sqlx::query_as::<_, (String,String,String,String,String)>("SELECT id::text,name,status,created_at::text,updated_at::text FROM organizations WHERE $1::timestamptz IS NULL OR (created_at,id)<($1::timestamptz,$2::uuid) ORDER BY created_at DESC,id DESC LIMIT $3")
        .bind(cursor.as_ref().map(|c|c.created_at.as_str())).bind(cursor.as_ref().map(|c|c.id.as_str())).bind(limit + 1).fetch_all(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let has_more = rows.len() as i64 > limit;
    let mut rows = rows;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let last = rows.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(
            encode_page_cursor(
                auth.mfa_key
                    .as_ref()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
                &scope,
                &last.3,
                &last.0,
            )
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    Ok(Json(OrganizationPage {
        data: rows.into_iter().map(organization_dto).collect(),
        next_cursor,
    }))
}

pub(super) async fn create_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OrganizationCreateRequest>,
) -> Result<(StatusCode, Json<CreatedOrganization>), StatusCode> {
    if !valid_name(&body.name) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let email = canonical_email(&body.admin_email)?;
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    require_platform_admin(&auth, &user).await?;
    let pool = auth
        .platform_pool
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let key = auth.outbox_key.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let org_id = uuid::Uuid::new_v4().to_string();
    let invite_id = uuid::Uuid::new_v4().to_string();
    let token = random_secret();
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let org = sqlx::query_as::<_,(String,String,String,String,String)>("INSERT INTO organizations(id,name) VALUES($1::uuid,$2) RETURNING id::text,name,status,created_at::text,updated_at::text")
        .bind(&org_id).bind(body.name.trim()).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let invitation = sqlx::query_as::<_,(String,i64)>("INSERT INTO invitations(id,tenant_id,email_canonical,role,token_hash,expires_at) VALUES($1::uuid,$2::uuid,$3,'organization_admin',$4,clock_timestamp()+interval '72 hours') RETURNING expires_at::text, extract(epoch from expires_at)::bigint")
        .bind(&invite_id).bind(&org_id).bind(&email).bind(digest(&token)).fetch_one(&mut *tx).await.map_err(|_| StatusCode::CONFLICT)?;
    let encrypted = email::encrypt_payload(&key, &token, invitation.1 as u64)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO email_outbox(id,tenant_id,recipient,template,encrypted_payload,retention_expires_at) VALUES($1::uuid,$2::uuid,$3,'invitation',$4,clock_timestamp()+interval '71 hours 59 minutes')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&org_id).bind(&email).bind(encrypted).execute(&mut *tx).await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,'organization.created','organization',$2::uuid,'success',$3)")
        .bind(format!("user:{}",user.user_id)).bind(&org_id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedOrganization {
            organization: organization_dto(org),
            invitation_id: invite_id,
        }),
    ))
}

pub(super) async fn set_organization_status(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    headers: HeaderMap,
    Json(body): Json<OrganizationStatusRequest>,
) -> Result<Json<OrganizationDto>, StatusCode> {
    if !matches!(body.status.as_str(), "active" | "suspended")
        || uuid::Uuid::parse_str(&id).is_err()
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    require_platform_admin(&auth, &user).await?;
    let pool = auth
        .platform_pool
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = sqlx::query_as::<_,(String,String,String,String,String)>("UPDATE organizations SET status=$2,updated_at=clock_timestamp() WHERE id=$1::uuid RETURNING id::text,name,status,created_at::text,updated_at::text")
        .bind(&id).bind(&body.status).fetch_optional(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    if body.status == "suspended" {
        sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE revoked_at IS NULL AND user_id IN (SELECT user_id FROM memberships WHERE tenant_id=$1::uuid)")
            .bind(&id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,$2,'organization',$3::uuid,'success',$4)")
        .bind(format!("user:{}",user.user_id)).bind(if body.status=="suspended" {"organization.suspended"} else {"organization.reactivated"}).bind(&id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(organization_dto(row)))
}

pub(super) async fn reissue_initial_admin_invitation(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    headers: HeaderMap,
    Json(body): Json<InvitationReissueRequest>,
) -> Result<(StatusCode, Json<InvitationDto>), StatusCode> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let email = canonical_email(&body.email)?;
    let auth = state.auth.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let user = session_user_csrf(&auth, &headers).await?;
    require_platform_admin(&auth, &user).await?;
    let pool = auth
        .platform_pool
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let key = auth.outbox_key.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let org_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM organizations WHERE id=$1::uuid FOR UPDATE")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if org_status.as_deref() != Some("active") {
        return Err(if org_status.is_none() {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::CONFLICT
        });
    }
    let has_admin: bool =
        sqlx::query_scalar("SELECT gps_tracking_organization_has_active_admin($1::uuid)")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if has_admin {
        return Err(StatusCode::CONFLICT);
    }
    sqlx::query("UPDATE invitations SET status='revoked',revoked_at=clock_timestamp() WHERE tenant_id=$1::uuid AND status='pending' AND role='organization_admin'").bind(&id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM email_outbox WHERE tenant_id=$1::uuid AND recipient=$2 AND template='invitation'").bind(&id).bind(&email).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    let token = random_secret();
    let invitation_id = uuid::Uuid::new_v4().to_string();
    let row=sqlx::query_as::<_,(String,String,String,String,String,String,String)>("INSERT INTO invitations(id,tenant_id,email_canonical,role,token_hash,expires_at) VALUES($1::uuid,$2::uuid,$3,'organization_admin',$4,clock_timestamp()+interval '72 hours') RETURNING id::text,tenant_id::text,email_canonical,role,status,expires_at::text,created_at::text")
        .bind(&invitation_id).bind(&id).bind(&email).bind(digest(&token)).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let expiry: i64 = sqlx::query_scalar(
        "SELECT extract(epoch from expires_at)::bigint FROM invitations WHERE id=$1::uuid",
    )
    .bind(&invitation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let encrypted = email::encrypt_payload(&key, &token, expiry as u64)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM email_outbox WHERE tenant_id=$1::uuid AND recipient=$2 AND template='invitation'")
        .bind(&id).bind(&email).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO email_outbox(id,tenant_id,recipient,template,encrypted_payload,retention_expires_at) VALUES($1::uuid,$2::uuid,$3,'invitation',$4,clock_timestamp()+interval '71 hours 59 minutes')").bind(uuid::Uuid::new_v4().to_string()).bind(&id).bind(&email).bind(encrypted).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,'organization.admin_invitation_reissued','organization',$2::uuid,'success',$3)").bind(format!("user:{}",user.user_id)).bind(&id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(invitation_dto(row))))
}
pub(super) async fn get_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OrganizationDto>, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user(&auth, &headers).await?;
    let tenant = require_tenant_member(&auth, &user, &headers).await?;
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row=sqlx::query_as::<_,(String,String,String,String,String)>("SELECT id::text,name,status,created_at::text,updated_at::text FROM organizations WHERE id=$1::uuid AND status='active'").bind(&tenant).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::FORBIDDEN)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(organization_dto(row)))
}

pub(super) async fn update_organization(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OrganizationNameRequest>,
) -> Result<Json<OrganizationDto>, StatusCode> {
    if !valid_name(&body.name) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    require_recent_auth(&auth, &user, false).await?;
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row=sqlx::query_as::<_,(String,String,String,String,String)>("UPDATE organizations SET name=$2,updated_at=clock_timestamp() WHERE id=$1::uuid AND status='active' RETURNING id::text,name,status,created_at::text,updated_at::text").bind(&tenant).bind(body.name.trim()).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::FORBIDDEN)?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES($1::uuid,$2,'organization.updated','organization',$1::uuid,'success',$3)").bind(&tenant).bind(format!("user:{}",user.user_id)).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(organization_dto(row)))
}

pub(super) async fn list_invitations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Result<Json<InvitationPage>, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let limit = page_limit(&query)?;
    let scope = format!("organization:{}:invitations", tenant);
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| {
            decode_page_cursor(
                auth.mfa_key
                    .as_ref()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
                &scope,
                value,
            )
        })
        .transpose()?;
    let rows=sqlx::query_as::<_,(String,String,String,String,String,String,String)>("SELECT id::text,tenant_id::text,email_canonical,role,status,expires_at::text,created_at::text FROM invitations WHERE tenant_id=$1::uuid AND ($2::timestamptz IS NULL OR (created_at,id)<($2::timestamptz,$3::uuid)) ORDER BY created_at DESC,id DESC LIMIT $4").bind(&tenant).bind(cursor.as_ref().map(|c|c.created_at.as_str())).bind(cursor.as_ref().map(|c|c.id.as_str())).bind(limit+1).fetch_all(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    let has_more = rows.len() as i64 > limit;
    let mut rows = rows;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let last = rows.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(
            encode_page_cursor(
                auth.mfa_key
                    .as_ref()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
                &scope,
                &last.6,
                &last.0,
            )
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(InvitationPage {
        data: rows.into_iter().map(invitation_dto).collect(),
        next_cursor,
    }))
}

pub(super) type MembershipRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

pub(super) fn membership_dto(row: MembershipRow) -> MembershipDto {
    MembershipDto {
        id: row.0,
        tenant_id: row.1,
        user_id: row.2,
        role: if row.3 == "organization_admin" {
            "admin".into()
        } else {
            row.3
        },
        status: row.4,
        created_at: row.5,
        updated_at: row.6,
        user: MembershipUserDto {
            email: row.7,
            display_name: Some(row.8),
        },
    }
}

pub(super) fn membership_list(grants: Vec<(String, String, String)>) -> GrantList {
    let mut grouped = std::collections::BTreeMap::<(String, String), Vec<String>>::new();
    for (resource_type, resource_id, permission) in grants {
        grouped
            .entry((resource_type, resource_id))
            .or_default()
            .push(permission);
    }
    GrantList {
        grants: grouped
            .into_iter()
            .map(|((resource_type, resource_id), mut permissions)| {
                permissions.sort();
                GrantDto {
                    resource_type,
                    resource_id,
                    permissions,
                }
            })
            .collect(),
    }
}

pub(super) async fn list_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Result<Json<MembershipPage>, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    let limit = page_limit(&query)?;
    let scope = format!("organization:{tenant}:members");
    let cursor = query
        .cursor
        .as_deref()
        .map(|value| {
            decode_page_cursor(
                auth.mfa_key
                    .as_ref()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
                &scope,
                value,
            )
        })
        .transpose()?;
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = sqlx::query_as::<_, MembershipRow>(
        "SELECT m.id::text,m.tenant_id::text,m.user_id::text,m.role,m.status,m.created_at::text,m.updated_at::text,u.email_canonical,u.display_name \
         FROM memberships m JOIN users u ON u.id=m.user_id WHERE m.tenant_id=$1::uuid \
         AND ($2::timestamptz IS NULL OR (m.created_at,m.id)<($2::timestamptz,$3::uuid)) \
         ORDER BY m.created_at DESC,m.id DESC LIMIT $4",
    ).bind(&tenant).bind(cursor.as_ref().map(|c| c.created_at.as_str()))
        .bind(cursor.as_ref().map(|c| c.id.as_str())).bind(limit+1)
        .fetch_all(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let has_more = rows.len() as i64 > limit;
    let mut rows = rows;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        let last = rows.last().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        Some(
            encode_page_cursor(
                auth.mfa_key
                    .as_ref()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?,
                &scope,
                &last.5,
                &last.0,
            )
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(MembershipPage {
        data: rows.into_iter().map(membership_dto).collect(),
        next_cursor,
    }))
}

pub(super) async fn mutate_membership(
    auth: &AuthState,
    actor: &AuthenticatedUser,
    tenant: &str,
    target_user: &str,
    requested_role: Option<&str>,
    requested_status: Option<&str>,
    remove: bool,
) -> Result<Option<MembershipDto>, StatusCode> {
    let role = requested_role.map(database_role).transpose()?;
    let saved = sqlx::query_as::<_, MembershipRow>(
        "SELECT id::text,tenant_id::text,user_id::text,role,status,created_at::text,updated_at::text,email_canonical,display_name \
         FROM gps_tracking_update_membership($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5,$6,$7)",
    ).bind(tenant).bind(&actor.user_id).bind(&actor.session_id).bind(target_user)
        .bind(role).bind(requested_status).bind(remove).fetch_one(&auth.pool).await
        .map_err(map_membership_error)?;
    Ok(Some(membership_dto(saved)))
}

pub(super) fn map_membership_error(error: sqlx::Error) -> StatusCode {
    match error
        .as_database_error()
        .and_then(|database| database.code())
    {
        Some(code) if code == "42501" => StatusCode::FORBIDDEN,
        Some(code) if code == "P0002" => StatusCode::NOT_FOUND,
        Some(code) if code == "23514" => StatusCode::CONFLICT,
        Some(code) if code == "22023" || code == "22P02" => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub(super) async fn update_member(
    State(state): State<AppState>,
    axum::extract::Path(target_user): axum::extract::Path<String>,
    headers: HeaderMap,
    Json(body): Json<MembershipPatchRequest>,
) -> Result<Json<MembershipDto>, StatusCode> {
    if uuid::Uuid::parse_str(&target_user).is_err()
        || (body.role.is_none() && body.status.is_none())
        || body
            .role
            .as_deref()
            .is_some_and(|role| database_role(role).is_err())
        || body
            .status
            .as_deref()
            .is_some_and(|status| !matches!(status, "active" | "suspended"))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let actor = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &actor, &headers).await?;
    require_recent_auth(&auth, &actor, false).await?;
    mutate_membership(
        &auth,
        &actor,
        &tenant,
        &target_user,
        body.role.as_deref(),
        body.status.as_deref(),
        false,
    )
    .await?
    .map(Json)
    .ok_or(StatusCode::INTERNAL_SERVER_ERROR)
}

pub(super) async fn remove_member(
    State(state): State<AppState>,
    axum::extract::Path(target_user): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    if uuid::Uuid::parse_str(&target_user).is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let actor = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &actor, &headers).await?;
    require_recent_auth(&auth, &actor, false).await?;
    mutate_membership(&auth, &actor, &tenant, &target_user, None, None, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_member_grants(
    State(state): State<AppState>,
    axum::extract::Path(target_user): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Result<Json<GrantList>, StatusCode> {
    if uuid::Uuid::parse_str(&target_user).is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let actor = session_user(&auth, &headers).await?;
    let tenant = require_tenant_member(&auth, &actor, &headers).await?;
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT resource_type,resource_id::text,permission FROM gps_tracking_list_member_grants($1::uuid,$2::uuid,$3::uuid,$4::uuid)",
    ).bind(&tenant).bind(&actor.user_id).bind(&actor.session_id).bind(&target_user).fetch_all(&auth.pool).await.map_err(map_membership_error)?;
    Ok(Json(membership_list(rows)))
}

pub(super) async fn replace_member_grants(
    State(state): State<AppState>,
    axum::extract::Path(target_user): axum::extract::Path<String>,
    headers: HeaderMap,
    Json(body): Json<GrantReplaceRequest>,
) -> Result<Json<GrantList>, StatusCode> {
    if uuid::Uuid::parse_str(&target_user).is_err() || body.grants.len() > 1000 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let actor = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &actor, &headers).await?;
    require_recent_auth(&auth, &actor, false).await?;
    let tenant_uuid = uuid::Uuid::parse_str(&tenant)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .to_string();
    let mut normalized = Vec::<(String, String, String)>::new();
    let mut seen_resources = std::collections::HashSet::new();
    for grant in &body.grants {
        let allowed: &[&str] = match grant.resource_type.as_str() {
            "organization" => &["targets:create", "devices:create"],
            "target" => &["read", "write"],
            "device" => &["read", "write", "credentials:write"],
            _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
        };
        let resource_id = uuid::Uuid::parse_str(&grant.resource_id)
            .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?
            .to_string();
        if grant.permissions.is_empty()
            || !seen_resources.insert((grant.resource_type.clone(), resource_id.clone()))
            || grant
                .permissions
                .iter()
                .any(|permission| !allowed.contains(&permission.as_str()))
            || grant
                .permissions
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != grant.permissions.len()
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        if grant.resource_type == "organization" && resource_id != tenant_uuid {
            return Err(StatusCode::NOT_FOUND);
        }
        for permission in &grant.permissions {
            normalized.push((
                grant.resource_type.clone(),
                resource_id.clone(),
                permission.clone(),
            ));
        }
    }
    if normalized.len() > 3000 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let grants_value =
        serde_json::to_value(&body.grants).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT resource_type,resource_id::text,permission FROM gps_tracking_replace_member_grants($1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::jsonb)",
    ).bind(&tenant).bind(&actor.user_id).bind(&actor.session_id).bind(&target_user).bind(grants_value)
        .fetch_all(&auth.pool).await.map_err(map_membership_error)?;
    Ok(Json(membership_list(rows)))
}

/// Evaluate the current membership, tenant and explicit object grants. Used by
/// management/tracking handlers instead of trusting a caller-supplied tenant ID.
pub async fn require_object_permission(
    pool: &PgPool,
    tenant: &str,
    user_id: &str,
    resource_type: &str,
    resource_id: &str,
    permission: &str,
) -> Result<(), StatusCode> {
    let valid = matches!(resource_type, "organization" | "target" | "device")
        && matches!(
            permission,
            "read" | "write" | "credentials:write" | "targets:create" | "devices:create"
        )
        && uuid::Uuid::parse_str(tenant).is_ok()
        && uuid::Uuid::parse_str(user_id).is_ok()
        && uuid::Uuid::parse_str(resource_id).is_ok();
    if !valid
        || (resource_type == "organization"
            && !matches!(permission, "targets:create" | "devices:create"))
        || (resource_type == "target" && !matches!(permission, "read" | "write"))
        || (resource_type == "device"
            && !matches!(permission, "read" | "write" | "credentials:write"))
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let allowed: bool = sqlx::query_scalar(
        "SELECT gps_tracking_member_has_permission($1::uuid,$2::uuid,$3,$4::uuid,$5)",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(resource_type)
    .bind(resource_id)
    .bind(permission)
    .fetch_one(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Check an application token against its live owner membership and explicit
/// allowlist; scopes are necessary but never preserve revoked owner access.
pub async fn require_application_token_owner_permission(
    pool: &PgPool,
    token_id: &str,
    tenant: &str,
    resource_type: &str,
    resource_id: &str,
    permission: &str,
) -> Result<(), StatusCode> {
    if !matches!(resource_type, "target" | "device")
        || !matches!(permission, "read" | "write")
        || uuid::Uuid::parse_str(token_id).is_err()
        || uuid::Uuid::parse_str(tenant).is_err()
        || uuid::Uuid::parse_str(resource_id).is_err()
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let allowed: bool = sqlx::query_scalar(
        "SELECT gps_tracking_application_token_owner_has_permission($1::uuid,$2::uuid,$3,$4::uuid,$5)",
    )
    .bind(token_id)
    .bind(tenant)
    .bind(resource_type)
    .bind(resource_id)
    .bind(permission)
    .fetch_one(pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if allowed {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Add the creating member's read/write grant in the same transaction as a
/// target/device insert; resource existence and tenant ownership are verified.
pub async fn grant_creator_object_access(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant: &str,
    user_id: &str,
    resource_type: &str,
    resource_id: &str,
) -> Result<(), sqlx::Error> {
    let tenant_id = uuid::Uuid::parse_str(tenant)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?
        .to_string();
    let resource_id = uuid::Uuid::parse_str(resource_id)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?
        .to_string();
    sqlx::query_scalar::<_, String>(
        "SELECT gps_tracking_grant_creator_object_access($1::uuid,$2::uuid,$3,$4::uuid)::text",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(resource_type)
    .bind(resource_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn write_invitation(
    auth: &AuthState,
    tenant: &str,
    user: &AuthenticatedUser,
    email: &str,
    role: &str,
    invitation_id: &str,
) -> Result<InvitationDto, StatusCode> {
    let role = database_role(role)?;
    let key = auth.outbox_key.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let token = random_secret();
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let result=sqlx::query_as::<_,(String,String,String,String,String,String,String)>("INSERT INTO invitations(id,tenant_id,email_canonical,role,token_hash,expires_at) SELECT $1::uuid,$2::uuid,$3,$4,$5,clock_timestamp()+interval '72 hours' WHERE EXISTS(SELECT 1 FROM organizations WHERE id=$2::uuid AND status='active') RETURNING id::text,tenant_id::text,email_canonical,role,status,expires_at::text,created_at::text")
        .bind(invitation_id).bind(tenant).bind(email).bind(role).bind(digest(&token)).fetch_optional(&mut *tx).await.map_err(|e| if e.as_database_error().is_some_and(|d| d.code().as_deref()==Some("23505")) {StatusCode::CONFLICT} else {StatusCode::INTERNAL_SERVER_ERROR})?.ok_or(StatusCode::FORBIDDEN)?;
    let expiry: i64 = sqlx::query_scalar(
        "SELECT extract(epoch from expires_at)::bigint FROM invitations WHERE id=$1::uuid",
    )
    .bind(invitation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let encrypted = email::encrypt_payload(&key, &token, expiry as u64)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM email_outbox WHERE tenant_id=$1::uuid AND recipient=$2 AND template='invitation'")
        .bind(tenant).bind(email).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO email_outbox(id,tenant_id,recipient,template,encrypted_payload,retention_expires_at) VALUES($1::uuid,$2::uuid,$3,'invitation',$4,clock_timestamp()+interval '71 hours 59 minutes')").bind(uuid::Uuid::new_v4().to_string()).bind(tenant).bind(email).bind(encrypted).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES($1::uuid,$2,'invitation.created','invitation',$3::uuid,'success',$4)").bind(tenant).bind(format!("user:{}",user.user_id)).bind(invitation_id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(invitation_dto(result))
}

pub(super) async fn create_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InvitationCreateRequest>,
) -> Result<(StatusCode, Json<InvitationDto>), StatusCode> {
    let email = canonical_email(&body.email)?;
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    require_recent_auth(&auth, &user, false).await?;
    let buckets = vec![
        (
            format!(
                "invite:recipient:{}",
                URL_SAFE_NO_PAD.encode(digest(&email))
            ),
            3,
        ),
        (format!("invite:actor:{}", user.user_id), 20),
    ];
    if !take_limited(&state.auth_rate_limit, &buckets) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let dto = write_invitation(
        &auth,
        &tenant,
        &user,
        &email,
        &body.role,
        &uuid::Uuid::new_v4().to_string(),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(dto)))
}

pub(super) async fn resend_invitation(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    headers: HeaderMap,
    Json(_body): Json<EmptyRequest>,
) -> Result<StatusCode, StatusCode> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let auth = state.auth.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    require_recent_auth(&auth, &user, false).await?;
    let key = auth.outbox_key.ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let invite=sqlx::query_as::<_,(String,String,String)>("SELECT email_canonical,role,id::text FROM invitations WHERE id=$1::uuid AND tenant_id=$2::uuid AND status='pending' AND expires_at>clock_timestamp() AND revoked_at IS NULL FOR UPDATE").bind(&id).bind(&tenant).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::NOT_FOUND)?;
    let buckets = vec![
        (
            format!(
                "invite:recipient:{}",
                URL_SAFE_NO_PAD.encode(digest(&invite.0))
            ),
            3,
        ),
        (format!("invite:actor:{}", user.user_id), 20),
    ];
    if !take_limited(&state.auth_rate_limit, &buckets) {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let token = random_secret();
    let expires:i64=sqlx::query_scalar("UPDATE invitations SET token_hash=$2,expires_at=clock_timestamp()+interval '72 hours' WHERE id=$1::uuid RETURNING extract(epoch from expires_at)::bigint").bind(&id).bind(digest(&token)).fetch_one(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    let encrypted = email::encrypt_payload(&key, &token, expires as u64)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM email_outbox WHERE tenant_id=$1::uuid AND recipient=$2 AND template='invitation'")
        .bind(&tenant).bind(&invite.0).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO email_outbox(id,tenant_id,recipient,template,encrypted_payload,retention_expires_at) VALUES($1::uuid,$2::uuid,$3,'invitation',$4,clock_timestamp()+interval '71 hours 59 minutes')").bind(uuid::Uuid::new_v4().to_string()).bind(&tenant).bind(&invite.0).bind(encrypted).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES($1::uuid,$2,'invitation.resent','invitation',$3::uuid,'success',$4)").bind(&tenant).bind(format!("user:{}",user.user_id)).bind(&id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::ACCEPTED)
}

pub(super) async fn revoke_invitation(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let tenant = require_tenant_admin(&auth, &user, &headers).await?;
    require_recent_auth(&auth, &user, false).await?;
    let mut tx = tracking_storage::begin_tenant_transaction(&auth.pool, &tenant)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let result=sqlx::query("UPDATE invitations SET status='revoked',revoked_at=clock_timestamp() WHERE id=$1::uuid AND tenant_id=$2::uuid AND status='pending' AND revoked_at IS NULL").bind(&id).bind(&tenant).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() != 1 {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES($1::uuid,$2,'invitation.revoked','invitation',$3::uuid,'success',$4)").bind(&tenant).bind(format!("user:{}",user.user_id)).bind(&id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn accept_invitation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InvitationAcceptRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let logged_in = cookie_value(&headers, "__Host-tracking_session").is_some();
    let existing_user = if logged_in {
        Some(session_user_csrf(&auth, &headers).await?)
    } else {
        require_pre_auth(&headers, &auth.origin)?;
        None
    };
    if body.token.is_empty() || body.token.len() > 256 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let identity=sqlx::query_as::<_,(String,String,String,String,String,i64)>("SELECT id::text,tenant_id::text,email_canonical,role,status,extract(epoch from expires_at)::bigint FROM gps_tracking_invitation_identity($1)").bind(digest(&body.token)).fetch_optional(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED)?;
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (user_id, is_new) = if let Some(user) = existing_user {
        if user.email != identity.2 {
            return Err(StatusCode::FORBIDDEN);
        }
        (user.user_id, false)
    } else {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email_canonical=$1)")
                .bind(&identity.2)
                .fetch_one(&auth.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if exists {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let password = body
            .password
            .as_deref()
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        let name = body
            .display_name
            .as_deref()
            .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
        if password.chars().count() < 15 || password.chars().count() > 128 || !valid_name(name) {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let permit = state
            .hash_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
        let owned = password.to_owned();
        let hash = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            hash_password(owned.as_bytes())
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users(id,email_canonical,display_name,password_hash,status) VALUES($1::uuid,$2,$3,$4,'active')").bind(&id).bind(&identity.2).bind(name.trim()).bind(hash).execute(&mut *tx).await.map_err(|e| if e.as_database_error().is_some_and(|d| d.code().as_deref()==Some("23505")){StatusCode::CONFLICT}else{StatusCode::INTERNAL_SERVER_ERROR})?;
        (id, true)
    };
    let row=sqlx::query_as::<_,(String,String,String)>("SELECT tenant_id::text,membership_id::text,membership_status FROM gps_tracking_accept_invitation($1,$2::uuid)").bind(digest(&body.token)).bind(&user_id).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED)?;
    if is_new {
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(serde_json::json!({"state":"login_required"})))
    } else {
        tracking_storage::set_tenant_context(&mut tx, &row.0)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let membership=sqlx::query_as::<_,(String,String,String,String,String,String,String)>("SELECT id::text,tenant_id::text,user_id::text,role,status,created_at::text,updated_at::text FROM memberships WHERE id=$1::uuid").bind(&row.1).fetch_one(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(Json(
            serde_json::json!({"state":"accepted","membership":{"id":membership.0,"tenant_id":membership.1,"user_id":membership.2,"role":if membership.3=="organization_admin" {"admin"} else {&membership.3},"status":membership.4,"created_at":membership.5,"updated_at":membership.6}}),
        ))
    }
}
