use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ForgotPasswordRequest {
    pub(super) email: String,
}

#[derive(Serialize)]
pub(super) struct ForgotPasswordResponse {
    pub(super) message: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PasswordResetRequest {
    pub(super) token: String,
    pub(super) new_password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryCompleteRequest {
    pub(super) token: String,
    pub(super) email_token: String,
    pub(super) password: String,
}

pub(super) async fn activate_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PasswordResetRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    require_pre_auth(&headers, &auth.origin)?;
    if body.token.is_empty() || !(15..=128).contains(&body.new_password.len()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let password = body.new_password;
    let hash = tokio::task::spawn_blocking(move || hash_password(password.as_bytes()))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let claim =
        tracking_storage::consume_account_token(&mut tx, &digest(&body.token), "activation")
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
    let updated = sqlx::query("UPDATE users SET password_hash=$1,status='active',auth_version=auth_version+1,updated_at=clock_timestamp() WHERE id=$2::uuid AND status='pending_activation'").bind(hash).bind(&claim.user_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if updated.rows_affected() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,'auth.activation_completed','user',$2::uuid,'success',$3)")
        .bind(format!("user:{}", claim.user_id)).bind(&claim.user_id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}

pub(super) async fn complete_operator_recovery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RecoveryCompleteRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    require_pre_auth(&headers, &auth.origin)?;
    if body.token.is_empty()
        || body.email_token.is_empty()
        || !(15..=128).contains(&body.password.len())
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let password = body.password;
    let hash = tokio::task::spawn_blocking(move || hash_password(password.as_bytes()))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let authorization =
        tracking_storage::consume_account_token(&mut tx, &digest(&body.token), "recovery")
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
    let email_proof = tracking_storage::consume_account_token(
        &mut tx,
        &digest(&body.email_token),
        "recovery_email",
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::UNAUTHORIZED)?;
    if authorization.user_id != email_proof.user_id {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let updated = sqlx::query("UPDATE users SET password_hash=$1,auth_version=auth_version+1,updated_at=clock_timestamp() WHERE id=$2::uuid AND status='active'").bind(hash).bind(&authorization.user_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if updated.rows_affected() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE user_id=$1::uuid AND revoked_at IS NULL").bind(&authorization.user_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM mfa_factors WHERE user_id=$1::uuid")
        .bind(&authorization.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id=$1::uuid")
        .bind(&authorization.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let challenge = random_secret();
    let expiry: String = sqlx::query_scalar("INSERT INTO auth_challenges(id,user_id,secret_hash,purpose,expires_at) VALUES($1::uuid,$2::uuid,$3,'mfa_recovery',clock_timestamp()+interval '5 minutes') RETURNING expires_at::text")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&authorization.user_id).bind(digest(&challenge)).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,'auth.operator_recovery_completed','user',$2::uuid,'success',$3)")
        .bind(format!("user:{}", authorization.user_id)).bind(&authorization.user_id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut response = (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(EnrollmentChallengeResponse {
            state: "mfa_enrollment_required",
            challenge_expires_at: expiry,
        }),
    )
        .into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        cookie("__Host-tracking_challenge", &challenge, 300)
            .parse()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    Ok(response)
}

pub(super) async fn forgot_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(remote): axum::extract::ConnectInfo<SocketAddr>,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    require_pre_auth(&headers, &auth.origin)?;
    if body.email.len() > 320 {
        return Err(StatusCode::BAD_REQUEST);
    }
    // Check feature configuration before querying the account so a missing key
    // cannot make eligible and unknown addresses behave differently.
    let outbox_key = auth.outbox_key.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    // Always return the same response. Token material is only persisted encrypted
    // in the outbox; neither it nor the recipient is written to application logs.
    let email = body.email.trim().to_ascii_lowercase();
    if email.parse::<lettre::Address>().is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let peer_ip = remote.ip();
    let ip = if auth.trusted_proxy_ip == Some(peer_ip) {
        headers
            .get("x-real-ip")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(peer_ip)
            .to_string()
    } else {
        peer_ip.to_string()
    };
    if !take_limited(
        &state.auth_rate_limit,
        &[
            (
                format!("recipient:{}", URL_SAFE_NO_PAD.encode(digest(&email))),
                3,
            ),
            (format!("ip:{}", URL_SAFE_NO_PAD.encode(digest(&ip))), 20),
        ],
    ) {
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "3600")],
            Json(ForgotPasswordResponse {
                message: "If eligible, instructions will be sent",
            }),
        )
            .into_response());
    }
    if !email.is_empty() {
        let mut tx = auth
            .pool
            .begin()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let user = sqlx::query_as::<_, (String, String)>(
            "SELECT id::text,email_canonical FROM users WHERE email_canonical=$1 AND status='active' FOR UPDATE",
        ).bind(&email).fetch_optional(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some((user_id, recipient)) = user {
            let token = random_secret();
            sqlx::query("UPDATE account_tokens SET revoked_at=clock_timestamp() WHERE user_id=$1::uuid AND purpose='password_reset' AND consumed_at IS NULL AND revoked_at IS NULL")
                .bind(&user_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let expires_at: i64 = sqlx::query_scalar("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'password_reset',$3,clock_timestamp()+interval '30 minutes') RETURNING extract(epoch FROM expires_at)::bigint")
                .bind(uuid::Uuid::new_v4().to_string()).bind(&user_id).bind(digest(&token)).fetch_one(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            // The outbox payload is encrypted at rest with a key separate from MFA.
            // The optional dedicated mailer process decrypts and delivers this record.
            let encrypted = email::encrypt_payload(&outbox_key, &token, expires_at as u64)
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            sqlx::query("INSERT INTO email_outbox(id,recipient,template,encrypted_payload,retention_expires_at) VALUES($1::uuid,$2,'password_reset',$3,clock_timestamp()+interval '71 hours 59 minutes')")
                .bind(uuid::Uuid::new_v4().to_string()).bind(recipient).bind(encrypted).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(ForgotPasswordResponse {
            message: "If eligible, instructions will be sent",
        }),
    )
        .into_response())
}

pub(super) fn take_limited(
    limiter: &Mutex<HashMap<String, VecDeque<Instant>>>,
    buckets: &[(String, usize)],
) -> bool {
    take_limited_for(limiter, buckets, Duration::from_secs(3600))
}

pub(super) fn take_limited_for(
    limiter: &Mutex<HashMap<String, VecDeque<Instant>>>,
    buckets: &[(String, usize)],
    window: Duration,
) -> bool {
    let Ok(mut state) = limiter.lock() else {
        return false;
    };
    let now = Instant::now();
    prune_limit_state(&mut state, now, window);
    if buckets.iter().any(|(key, limit)| {
        state
            .get(key)
            .is_some_and(|entries| entries.len() >= *limit)
    }) {
        return false;
    }
    if state.len()
        + buckets
            .iter()
            .filter(|(key, _)| !state.contains_key(key))
            .count()
        > 50_000
    {
        return false;
    }
    for (key, _) in buckets {
        state.entry(key.clone()).or_default().push_back(now);
    }
    true
}

pub(super) fn is_limited_for(
    limiter: &Mutex<HashMap<String, VecDeque<Instant>>>,
    key: &str,
    limit: usize,
    window: Duration,
) -> bool {
    let Ok(mut state) = limiter.lock() else {
        return true;
    };
    let now = Instant::now();
    prune_limit_state(&mut state, now, window);
    state.get(key).is_some_and(|entries| entries.len() >= limit)
}

pub(super) fn clear_limited_bucket(limiter: &Mutex<HashMap<String, VecDeque<Instant>>>, key: &str) {
    if let Ok(mut state) = limiter.lock() {
        state.remove(key);
    }
}

fn prune_limit_state(
    state: &mut HashMap<String, VecDeque<Instant>>,
    now: Instant,
    window: Duration,
) {
    state.retain(|_, entries| {
        while entries
            .front()
            .is_some_and(|seen| now.duration_since(*seen) >= window)
        {
            entries.pop_front();
        }
        !entries.is_empty()
    });
}

pub(super) async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PasswordResetRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    require_pre_auth(&headers, &auth.origin)?;
    if body.token.is_empty() || !(15..=128).contains(&body.new_password.len()) {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let password = body.new_password;
    let hash = tokio::task::spawn_blocking(move || hash_password(password.as_bytes()))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let claim =
        tracking_storage::consume_account_token(&mut tx, &digest(&body.token), "password_reset")
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
    let updated = sqlx::query("UPDATE users SET password_hash=$1,auth_version=auth_version+1,updated_at=clock_timestamp() WHERE id=$2::uuid AND status='active'")
        .bind(hash).bind(&claim.user_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if updated.rows_affected() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE user_id=$1::uuid AND revoked_at IS NULL")
        .bind(&claim.user_id).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,'auth.password_reset','user',$2::uuid,'success',$3)")
        .bind(format!("user:{}", claim.user_id)).bind(&claim.user_id).bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    ))
}

pub(super) async fn auth_no_store(request: axum::http::Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

#[derive(Serialize)]
pub(super) struct CsrfResponse {
    pub(super) csrf_token: String,
}

#[derive(Serialize)]
pub(super) struct AuthMeMembershipUser {
    pub(super) email: String,
    pub(super) display_name: Option<String>,
}

#[derive(Serialize)]
pub(super) struct AuthMeMembership {
    pub(super) id: String,
    pub(super) tenant_id: String,
    pub(super) user_id: String,
    pub(super) role: String,
    pub(super) status: String,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) user: AuthMeMembershipUser,
}

#[derive(Serialize)]
pub(super) struct AuthMeResponse {
    pub(super) user: PublicUser,
    pub(super) platform_admin: bool,
    pub(super) mfa_required: bool,
    pub(super) mfa_enabled: bool,
    pub(super) memberships: Vec<AuthMeMembership>,
}

#[derive(Serialize)]
pub(super) struct AuthSessionDto {
    pub(super) id: String,
    pub(super) created_at: String,
    pub(super) last_used_at: Option<String>,
    pub(super) expires_at: String,
    pub(super) current: bool,
    pub(super) device_label: &'static str,
}

#[derive(Serialize)]
pub(super) struct AuthSessionPage {
    pub(super) data: Vec<AuthSessionDto>,
    pub(super) next_cursor: Option<String>,
}

pub(super) async fn get_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthMeResponse>, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    if !same_origin_or_same_origin_referer(&headers, &auth.origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    let session = authenticate_cookie_session(&auth.pool, &headers, true).await?;
    let (id, email, display_name, status, platform_admin, created_at, updated_at) =
        sqlx::query_as::<_, (String, String, String, String, bool, String, String)>(
            "SELECT id::text,email_canonical,display_name,status,platform_admin,created_at::text,updated_at::text \
         FROM users WHERE id=$1::uuid AND status='active'",
        )
        .bind(&session.user_id)
        .fetch_optional(&auth.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let mfa_required: bool =
        sqlx::query_scalar("SELECT public.gps_tracking_user_requires_mfa($1::uuid)")
            .bind(&session.user_id)
            .fetch_one(&auth.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mfa_enabled: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL)",
    )
    .bind(&session.user_id)
    .fetch_one(&auth.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let membership_rows = sqlx::query_as::<
        _,
        (String, String, String, String, String, String, String),
    >(
        "SELECT id::text,tenant_id::text,user_id::text,role,status,created_at::text,updated_at::text \
         FROM public.gps_tracking_list_user_memberships($1::uuid)",
    )
    .bind(&session.user_id)
    .fetch_all(&auth.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let memberships = membership_rows
        .into_iter()
        .map(
            |(id, tenant_id, user_id, role, status, created_at, updated_at)| AuthMeMembership {
                id,
                tenant_id,
                user_id,
                role: if role == "organization_admin" {
                    "admin".to_owned()
                } else {
                    role
                },
                status,
                created_at,
                updated_at,
                user: AuthMeMembershipUser {
                    email: email.clone(),
                    display_name: Some(display_name.clone()),
                },
            },
        )
        .collect();

    Ok(Json(AuthMeResponse {
        user: PublicUser {
            id,
            email,
            display_name,
            status,
            created_at,
            updated_at,
        },
        platform_admin,
        mfa_required,
        mfa_enabled,
        memberships,
    }))
}

pub(super) async fn list_auth_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> Result<Json<AuthSessionPage>, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user(&auth, &headers).await?;
    let limit = page_limit(&query)?;
    let scope = format!("auth-sessions:user:{}:created_at_desc", user.user_id);
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
    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT s.id::text,s.created_at::text,s.last_interactive_at::text,s.expires_at::text \
         FROM auth_sessions s JOIN users u ON u.id=s.user_id \
         WHERE s.user_id=$1::uuid AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() \
         AND s.last_interactive_at>clock_timestamp()-interval '30 minutes' \
         AND s.auth_version=u.auth_version AND u.status='active' \
         AND ($2::timestamptz IS NULL OR (s.created_at,s.id)<($2::timestamptz,$3::uuid)) \
         ORDER BY s.created_at DESC,s.id DESC LIMIT $4",
    )
    .bind(&user.user_id)
    .bind(cursor.as_ref().map(|value| value.created_at.as_str()))
    .bind(cursor.as_ref().map(|value| value.id.as_str()))
    .bind(limit + 1)
    .fetch_all(&auth.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut rows = rows;
    let has_more = rows.len() as i64 > limit;
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
                &last.1,
                &last.0,
            )
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };
    Ok(Json(AuthSessionPage {
        data: rows
            .into_iter()
            .map(
                |(id, created_at, last_used_at, expires_at)| AuthSessionDto {
                    current: id == user.session_id,
                    id,
                    created_at,
                    last_used_at: Some(last_used_at),
                    expires_at,
                    device_label: "Browser session",
                },
            )
            .collect(),
        next_cursor,
    }))
}

pub(super) async fn get_csrf(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    if !same_origin_or_same_origin_referer(&headers, &auth.origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    let token = random_secret();
    let mut clear_stale_session_cookie = false;
    if let Some(session_secret) = cookie_value(&headers, "__Host-tracking_session") {
        let session_id: Option<String> = sqlx::query_scalar(
            "SELECT s.id::text FROM auth_sessions s JOIN users u ON u.id=s.user_id \
             WHERE s.secret_hash=$1 AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() \
             AND s.last_interactive_at>clock_timestamp()-interval '30 minutes' AND s.auth_version=u.auth_version",
        )
        .bind(digest(session_secret))
        .fetch_optional(&auth.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(session_id) = session_id {
            sqlx::query(
                "UPDATE auth_sessions SET csrf_hash=$1 WHERE id=$2::uuid AND revoked_at IS NULL",
            )
            .bind(digest(&token))
            .bind(session_id)
            .execute(&auth.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut response = (
                StatusCode::OK,
                [(header::CACHE_CONTROL, "no-store")],
                Json(CsrfResponse {
                    csrf_token: token.clone(),
                }),
            )
                .into_response();
            // Keep the short-lived pre-auth CSRF cookie synchronized as well. A
            // portal tab can reach the login form while a session cookie is still
            // present (for example during an auth-state transition); login checks
            // this cookie/header pair independently of the session CSRF hash.
            response.headers_mut().append(
                header::SET_COOKIE,
                cookie("__Host-tracking_pre_auth", &token, 300)
                    .parse()
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            );
            return Ok(response);
        }
        // A stale session cookie must not prevent the anonymous login flow from
        // obtaining its pre-auth CSRF token. Clear it and continue as anonymous.
        clear_stale_session_cookie = true;
    }
    let mut response = (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(CsrfResponse {
            csrf_token: token.clone(),
        }),
    )
        .into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        cookie("__Host-tracking_pre_auth", &token, 300)
            .parse()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    if clear_stale_session_cookie {
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie("__Host-tracking_session", "", 0)
                .parse()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
    }
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LoginRequest {
    pub(super) email: String,
    pub(super) password: String,
}

#[derive(Serialize)]
pub(super) struct PublicUser {
    pub(super) id: String,
    pub(super) email: String,
    pub(super) display_name: String,
    pub(super) status: String,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(super) enum LoginResult {
    Authenticated { user: PublicUser },
    MfaRequired { challenge_expires_at: String },
    MfaEnrollmentRequired { challenge_expires_at: String },
}

pub(super) async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    if !same_origin(&headers, &auth.origin) || !valid_pre_auth_csrf(&headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    let email_input = body.email.trim().to_ascii_lowercase();
    let account_key = format!(
        "login:failure:{}",
        URL_SAFE_NO_PAD.encode(digest(&email_input))
    );
    if is_limited_for(
        &state.auth_rate_limit,
        &account_key,
        5,
        Duration::from_secs(15 * 60),
    ) {
        return Ok((
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "900")],
        )
            .into_response());
    }
    if body.password.chars().count() < 15
        || body.password.chars().count() > 128
        || body.email.len() > 320
    {
        let _ = take_limited_for(
            &state.auth_rate_limit,
            &[(account_key, 5)],
            Duration::from_secs(15 * 60),
        );
        return Err(StatusCode::UNAUTHORIZED);
    }
    let email = email_input;
    let found = sqlx::query_as::<_, (String, String, String, String, String, String, i64)>(
        "SELECT id::text,email_canonical,display_name,status,password_hash,created_at::text,auth_version FROM users WHERE email_canonical=$1")
        .bind(email).fetch_optional(&auth.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let dummy = "$argon2id$v=19$m=65536,t=3,p=1$c29tZXNhbHQxMjM0NQ$W7QmUqYdPXNwWv7H9WEy7vL7GTqP7lP6zJ0YG8m6f4Q";
    let (id, email, display_name, status, password_hash, created_at, auth_version) =
        found.clone().unwrap_or_else(|| {
            (
                String::new(),
                String::new(),
                String::new(),
                "disabled".into(),
                dummy.into(),
                String::new(),
                0,
            )
        });
    let hash_permit = state
        .hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let valid_password = tokio::task::spawn_blocking(move || {
        let _permit = hash_permit;
        use argon2::{Argon2, PasswordHash, PasswordVerifier};
        PasswordHash::new(&password_hash).ok().is_some_and(|hash| {
            Argon2::default()
                .verify_password(body.password.as_bytes(), &hash)
                .is_ok()
        })
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid_password || found.is_none() || status != "active" {
        let _ = take_limited_for(
            &state.auth_rate_limit,
            &[(account_key, 5)],
            Duration::from_secs(15 * 60),
        );
        return Err(StatusCode::UNAUTHORIZED);
    }
    clear_limited_bucket(&state.auth_rate_limit, &account_key);
    let required: bool = sqlx::query_scalar(
        "SELECT public.gps_tracking_user_requires_mfa($1::uuid) OR EXISTS(SELECT 1 FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL)")
        .bind(&id).fetch_one(&auth.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if required {
        let challenge = random_secret();
        let digest = digest(&challenge);
        let cid = uuid::Uuid::new_v4();
        let purpose = if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL)").bind(&id).fetch_one(&auth.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? { "mfa_login" } else { "mfa_enrollment" };
        sqlx::query("INSERT INTO auth_challenges(id,user_id,secret_hash,purpose,expires_at) VALUES($1::uuid,$2::uuid,$3,$4,clock_timestamp()+interval '5 minutes')")
            .bind(cid.to_string()).bind(&id).bind(digest).bind(purpose).execute(&auth.pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let expires: String =
            sqlx::query_scalar("SELECT (clock_timestamp()+interval '5 minutes')::text")
                .fetch_one(&auth.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let state = if purpose == "mfa_login" {
            LoginResult::MfaRequired {
                challenge_expires_at: expires,
            }
        } else {
            LoginResult::MfaEnrollmentRequired {
                challenge_expires_at: expires,
            }
        };
        let mut response = (
            StatusCode::OK,
            [(header::CACHE_CONTROL, "no-store")],
            Json(state),
        )
            .into_response();
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie("__Host-tracking_challenge", &challenge, 300)
                .parse()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
        return Ok(response);
    }
    issue_session(
        &auth.pool,
        &headers,
        &id,
        &email,
        &display_name,
        &created_at,
        auth_version,
    )
    .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MfaCodeRequest {
    pub(super) code: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryCodeRequest {
    pub(super) recovery_code: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReauthenticateRequest {
    pub(super) password: String,
    pub(super) code: Option<String>,
}

#[derive(Serialize)]
pub(super) struct MfaEnrollmentResponse {
    pub(super) secret: String,
    pub(super) otpauth_uri: String,
}

#[derive(Serialize)]
pub(super) struct RecoveryCodesResponse {
    pub(super) recovery_codes: Vec<String>,
}

pub(super) struct ChallengeRow {
    pub(super) id: String,
    pub(super) user_id: String,
    pub(super) purpose: String,
}

#[derive(Serialize)]
pub(super) struct EnrollmentChallengeResponse {
    pub(super) state: &'static str,
    pub(super) challenge_expires_at: String,
}

pub(super) fn current_totp_step() -> Result<u64, StatusCode> {
    use std::time::{SystemTime, UNIX_EPOCH};
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_secs()
        / 30)
}

pub(super) fn require_pre_auth(headers: &HeaderMap, origin: &str) -> Result<(), StatusCode> {
    if !same_origin(headers, origin) || !valid_pre_auth_csrf(headers) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

pub(super) fn challenge_secret(headers: &HeaderMap) -> Result<&str, StatusCode> {
    cookie_value(headers, "__Host-tracking_challenge").ok_or(StatusCode::UNAUTHORIZED)
}

pub(super) async fn challenge_row(pool: &PgPool, secret: &str) -> Result<ChallengeRow, StatusCode> {
    sqlx::query_as::<_, (String, String, String, i16)>(
        "SELECT id::text,user_id::text,purpose,attempt_count FROM auth_challenges \
         WHERE secret_hash=$1 AND consumed_at IS NULL AND expires_at>clock_timestamp() AND attempt_count<5")
        .bind(digest(secret)).fetch_optional(pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map(|(id,user_id,purpose,_attempts)| ChallengeRow{id,user_id,purpose})
        .ok_or(StatusCode::UNAUTHORIZED)
}

pub(super) async fn verify_mfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MfaCodeRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    require_pre_auth(&headers, &auth.origin)?;
    let secret = challenge_secret(&headers)?;
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = sqlx::query_as::<_, (String,String,String,i16)>(
        "SELECT id::text,user_id::text,purpose,attempt_count FROM auth_challenges WHERE secret_hash=$1 AND consumed_at IS NULL AND expires_at>clock_timestamp() AND attempt_count<5 FOR UPDATE")
        .bind(digest(secret)).fetch_optional(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED)?;
    if row.2 != "mfa_login" {
        return Err(StatusCode::FORBIDDEN);
    }
    let factor = sqlx::query_as::<_, (Vec<u8>,Vec<u8>,i16,Option<i64>)>("SELECT encrypted_secret,nonce,key_version,last_accepted_step FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL FOR UPDATE")
        .bind(&row.1).fetch_optional(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some((ciphertext, nonce, key_version, last)) = factor else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let key = mfa_key_for_version(&auth, key_version).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let plaintext =
        mfa::decrypt(&key, &ciphertext, &nonce).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let step = mfa::matching_step(&plaintext, &body.code, current_totp_step()?, last);
    let Some(step) = step else {
        sqlx::query("UPDATE auth_challenges SET attempt_count=LEAST(attempt_count+1,5),consumed_at=CASE WHEN attempt_count+1>=5 THEN clock_timestamp() ELSE consumed_at END WHERE id=$1::uuid").bind(&row.0).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Err(StatusCode::UNAUTHORIZED);
    };
    let update = sqlx::query("UPDATE mfa_factors SET last_accepted_step=$1 WHERE user_id=$2::uuid AND enabled_at IS NOT NULL AND (last_accepted_step IS NULL OR last_accepted_step<$1)").bind(step as i64).bind(&row.1).execute(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if update.rows_affected() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    sqlx::query("UPDATE auth_challenges SET consumed_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&row.0)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user = sqlx::query_as::<_, (String,String,String,String,i64)>("SELECT id::text,email_canonical,display_name,created_at::text,auth_version FROM users WHERE id=$1::uuid AND status='active'").bind(&row.1).fetch_optional(&mut *tx).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    issue_session(
        &auth.pool, &headers, &user.0, &user.1, &user.2, &user.3, user.4,
    )
    .await
}

pub(super) async fn begin_mfa_enrollment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_body): Json<EmptyRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let key = auth
        .mfa_key
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let (user_id, restricted) = if cookie_value(&headers, "__Host-tracking_session").is_some() {
        if !same_origin(&headers, &auth.origin) {
            return Err(StatusCode::FORBIDDEN);
        }
        let user = authenticate_cookie_session(&auth.pool, &headers, true).await?;
        let csrf = headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::FORBIDDEN)?;
        let recent: bool=sqlx::query_scalar("SELECT csrf_hash=$2 AND reauthenticated_at>clock_timestamp()-interval '5 minutes' FROM auth_sessions WHERE id=$1::uuid AND revoked_at IS NULL")
            .bind(&user.session_id).bind(digest(csrf)).fetch_optional(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.unwrap_or(false);
        if !recent {
            return Err(StatusCode::FORBIDDEN);
        }
        (user.user_id, false)
    } else {
        require_pre_auth(&headers, &auth.origin)?;
        let challenge = challenge_row(&auth.pool, challenge_secret(&headers)?).await?;
        if challenge.purpose != "mfa_enrollment" && challenge.purpose != "mfa_recovery" {
            return Err(StatusCode::FORBIDDEN);
        }
        (challenge.user_id, true)
    };
    let raw = mfa::new_secret();
    let encoded = mfa::base32_encode(&raw);
    let (encrypted, nonce) = mfa::encrypt(key, &raw).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query(
        "DELETE FROM mfa_factors WHERE user_id=$1::uuid AND pending_expires_at IS NOT NULL",
    )
    .bind(&user_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,pending_expires_at) VALUES($1::uuid,$2::uuid,$3,$4,$5,clock_timestamp()+interval '10 minutes')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&user_id).bind(encrypted).bind(nonce).bind(auth.mfa_key_version).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    if restricted {
        let secret = challenge_secret(&headers)?;
        let result=sqlx::query("UPDATE auth_challenges SET attempt_count=attempt_count+1 WHERE secret_hash=$1 AND consumed_at IS NULL AND attempt_count<5").bind(digest(secret)).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
        if result.rows_affected() != 1 {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let uri = format!(
        "otpauth://totp/BackendTracker:{}?secret={}&issuer=BackendTracker&algorithm=SHA1&digits=6&period=30",
        urlencoding_for_uri(&user_id),
        encoded
    );
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(MfaEnrollmentResponse {
            secret: encoded,
            otpauth_uri: uri,
        }),
    )
        .into_response())
}

pub(super) fn urlencoding_for_uri(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace(':', "%3A")
        .replace('/', "%2F")
        .replace('?', "%3F")
        .replace('#', "%23")
        .replace(' ', "%20")
}

pub(super) async fn session_user(
    auth: &AuthState,
    headers: &HeaderMap,
) -> Result<AuthenticatedUser, StatusCode> {
    // This helper is only used by read-only GET handlers. Same-origin GET
    // fetches may omit Origin, so require an exact same-origin Referer fallback.
    // Mutations use session_user_csrf and still require exact Origin + CSRF.
    if !same_origin_or_same_origin_referer(headers, &auth.origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    authenticate_cookie_session(&auth.pool, headers, true).await
}

pub(super) async fn session_user_csrf(
    auth: &AuthState,
    headers: &HeaderMap,
) -> Result<AuthenticatedUser, StatusCode> {
    if !same_origin(headers, &auth.origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    let user = authenticate_cookie_session(&auth.pool, headers, true).await?;
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    let valid: bool = sqlx::query_scalar(
        "SELECT csrf_hash=$2 FROM auth_sessions WHERE id=$1::uuid AND revoked_at IS NULL",
    )
    .bind(&user.session_id)
    .bind(digest(csrf))
    .fetch_optional(&auth.pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .unwrap_or(false);
    if !valid {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(user)
}

pub(super) fn new_recovery_codes() -> Vec<String> {
    (0..10).map(|_| mfa::recovery_code()).collect()
}

pub(super) async fn confirm_mfa_enrollment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MfaCodeRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let (user_id, challenge) = if cookie_value(&headers, "__Host-tracking_session").is_some() {
        (session_user_csrf(&auth, &headers).await?.user_id, None)
    } else {
        require_pre_auth(&headers, &auth.origin)?;
        let secret = challenge_secret(&headers)?;
        let row = challenge_row(&auth.pool, secret).await?;
        if row.purpose != "mfa_enrollment" && row.purpose != "mfa_recovery" {
            return Err(StatusCode::FORBIDDEN);
        }
        (row.user_id, Some((row.id, row.purpose)))
    };
    let step = current_totp_step()?;
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row=sqlx::query_as::<_,(String,Vec<u8>,Vec<u8>,i16,Option<i64>)>("SELECT id::text,encrypted_secret,nonce,key_version,last_accepted_step FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NULL AND pending_expires_at>clock_timestamp() FOR UPDATE")
        .bind(&user_id).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED)?;
    let key = mfa_key_for_version(&auth, row.3).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let secret = mfa::decrypt(&key, &row.1, &row.2).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let matched = mfa::matching_step(&secret, &body.code, step, row.4);
    let Some(matched) = matched else {
        if let Some((id, _)) = &challenge {
            sqlx::query("UPDATE auth_challenges SET attempt_count=LEAST(attempt_count+1,5),consumed_at=CASE WHEN attempt_count+1>=5 THEN clock_timestamp() ELSE consumed_at END WHERE id=$1::uuid").bind(id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Err(StatusCode::UNAUTHORIZED);
    };
    if let Some((id, purpose)) = &challenge {
        if purpose == "mfa_recovery" {
            sqlx::query(
                "DELETE FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL",
            )
            .bind(&user_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        let consumed=sqlx::query("UPDATE auth_challenges SET consumed_at=clock_timestamp() WHERE id=$1::uuid AND consumed_at IS NULL AND attempt_count<5").bind(id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
        if consumed.rows_affected() != 1 {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    sqlx::query("UPDATE mfa_factors SET enabled_at=clock_timestamp(),pending_expires_at=NULL,last_accepted_step=$1 WHERE id=$2::uuid")
        .bind(matched as i64).bind(&row.0).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query_scalar::<_, i32>(
        "SELECT public.gps_tracking_activate_pending_mfa_memberships($1::uuid)",
    )
    .bind(&user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE recovery_codes SET consumed_at=clock_timestamp() WHERE user_id=$1::uuid AND consumed_at IS NULL")
        .bind(&user_id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    let codes = new_recovery_codes();
    for code in &codes {
        sqlx::query(
            "INSERT INTO recovery_codes(id,user_id,code_hash) VALUES($1::uuid,$2::uuid,$3)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user_id)
        .bind(digest(code))
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let user=sqlx::query_as::<_,(String,String,String,String,i64)>("SELECT id::text,email_canonical,display_name,created_at::text,auth_version FROM users WHERE id=$1::uuid").bind(&user_id).fetch_one(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    if challenge
        .as_ref()
        .is_some_and(|(_, purpose)| purpose == "mfa_recovery")
    {
        sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE user_id=$1::uuid AND revoked_at IS NULL").bind(&user_id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut response = (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(RecoveryCodesResponse {
            recovery_codes: codes,
        }),
    )
        .into_response();
    if challenge
        .as_ref()
        .is_some_and(|(_, purpose)| purpose == "mfa_enrollment")
    {
        let session_secret = random_secret();
        let csrf = random_secret();
        sqlx::query("INSERT INTO auth_sessions(id,user_id,secret_hash,csrf_hash,auth_version,expires_at) VALUES($1::uuid,$2::uuid,$3,$4,$5,clock_timestamp()+interval '12 hours')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&user.0).bind(digest(&session_secret)).bind(digest(&csrf)).bind(user.4).execute(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
        response.headers_mut().append(
            header::SET_COOKIE,
            cookie("__Host-tracking_session", &session_secret, 43200)
                .parse()
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
        response.headers_mut().append(
            header::SET_COOKIE,
            "__Host-tracking_challenge=; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
                .parse()
                .unwrap(),
        );
    } else if challenge
        .as_ref()
        .is_some_and(|(_, purpose)| purpose == "mfa_recovery")
    {
        response.headers_mut().append(
            header::SET_COOKIE,
            "__Host-tracking_challenge=; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
                .parse()
                .unwrap(),
        );
    }
    Ok(response)
}

pub(super) async fn recover_mfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RecoveryCodeRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    require_pre_auth(&headers, &auth.origin)?;
    let original = challenge_secret(&headers)?;
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row=sqlx::query_as::<_,(String,String,String)>("SELECT id::text,user_id::text,purpose FROM auth_challenges WHERE secret_hash=$1 AND consumed_at IS NULL AND expires_at>clock_timestamp() AND attempt_count<5 FOR UPDATE").bind(digest(original)).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED)?;
    if row.2 != "mfa_login" {
        return Err(StatusCode::FORBIDDEN);
    }
    let consumed=sqlx::query("UPDATE recovery_codes SET consumed_at=clock_timestamp() WHERE user_id=$1::uuid AND code_hash=$2 AND consumed_at IS NULL").bind(&row.1).bind(digest(body.recovery_code.trim())).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    if consumed.rows_affected() != 1 {
        sqlx::query("UPDATE auth_challenges SET attempt_count=LEAST(attempt_count+1,5),consumed_at=CASE WHEN attempt_count+1>=5 THEN clock_timestamp() ELSE consumed_at END WHERE id=$1::uuid").bind(&row.0).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Err(StatusCode::UNAUTHORIZED);
    }
    sqlx::query("UPDATE auth_challenges SET consumed_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&row.0)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE user_id=$1::uuid AND revoked_at IS NULL").bind(&row.1).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    let restricted = random_secret();
    let expires: String =
        sqlx::query_scalar("SELECT (clock_timestamp()+interval '5 minutes')::text")
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("INSERT INTO auth_challenges(id,user_id,secret_hash,purpose,expires_at) VALUES($1::uuid,$2::uuid,$3,'mfa_recovery',clock_timestamp()+interval '5 minutes')").bind(uuid::Uuid::new_v4().to_string()).bind(&row.1).bind(digest(&restricted)).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut response = (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(EnrollmentChallengeResponse {
            state: "mfa_enrollment_required",
            challenge_expires_at: expires,
        }),
    )
        .into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        cookie("__Host-tracking_challenge", &restricted, 300)
            .parse()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    Ok(response)
}

pub(super) async fn reauthenticate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReauthenticateRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    if body.password.chars().count() < 15 || body.password.chars().count() > 128 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let password_hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1::uuid AND status='active'")
            .bind(&user.user_id)
            .fetch_optional(&auth.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(password_hash) = password_hash else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let permit = state
        .hash_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let password = body.password.clone();
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        use argon2::{Argon2, PasswordHash, PasswordVerifier};
        PasswordHash::new(&password_hash).ok().is_some_and(|ph| {
            Argon2::default()
                .verify_password(password.as_bytes(), &ph)
                .is_ok()
        })
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let required:bool=sqlx::query_scalar("SELECT public.gps_tracking_user_requires_mfa($1::uuid) OR EXISTS(SELECT 1 FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL)").bind(&user.user_id).fetch_one(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    if required {
        let code = body.code.as_deref().ok_or(StatusCode::FORBIDDEN)?;
        let mut tx = auth
            .pool
            .begin()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let factor=sqlx::query_as::<_,(Vec<u8>,Vec<u8>,i16,Option<i64>)>("SELECT encrypted_secret,nonce,key_version,last_accepted_step FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL FOR UPDATE").bind(&user.user_id).fetch_optional(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::FORBIDDEN)?;
        let key = mfa_key_for_version(&auth, factor.2).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
        let plaintext =
            mfa::decrypt(&key, &factor.0, &factor.1).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let step = mfa::matching_step(&plaintext, code, current_totp_step()?, factor.3)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let changed=sqlx::query("UPDATE mfa_factors SET last_accepted_step=$1 WHERE user_id=$2::uuid AND (last_accepted_step IS NULL OR last_accepted_step<$1)").bind(step as i64).bind(&user.user_id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
        if changed.rows_affected() != 1 {
            return Err(StatusCode::UNAUTHORIZED);
        }
        sqlx::query("UPDATE auth_sessions SET reauthenticated_at=clock_timestamp() WHERE id=$1::uuid AND revoked_at IS NULL").bind(&user.session_id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
        tx.commit()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        sqlx::query("UPDATE auth_sessions SET reauthenticated_at=clock_timestamp() WHERE id=$1::uuid AND revoked_at IS NULL").bind(&user.session_id).execute(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response())
}

pub(super) async fn rotate_recovery_codes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_body): Json<EmptyRequest>,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let recent:bool=sqlx::query_scalar("SELECT reauthenticated_at>clock_timestamp()-interval '5 minutes' FROM auth_sessions WHERE id=$1::uuid").bind(&user.session_id).fetch_optional(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.unwrap_or(false);
    if !recent {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE recovery_codes SET consumed_at=clock_timestamp() WHERE user_id=$1::uuid AND consumed_at IS NULL").bind(&user.user_id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    let codes = new_recovery_codes();
    for code in &codes {
        sqlx::query(
            "INSERT INTO recovery_codes(id,user_id,code_hash) VALUES($1::uuid,$2::uuid,$3)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user.user_id)
        .bind(digest(code))
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(RecoveryCodesResponse {
            recovery_codes: codes,
        }),
    )
        .into_response())
}

pub(super) async fn disable_mfa(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let user = session_user_csrf(&auth, &headers).await?;
    let recent:bool=sqlx::query_scalar("SELECT reauthenticated_at>clock_timestamp()-interval '5 minutes' FROM auth_sessions WHERE id=$1::uuid").bind(&user.session_id).fetch_optional(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.unwrap_or(false);
    if !recent {
        return Err(StatusCode::FORBIDDEN);
    }
    let required: bool =
        sqlx::query_scalar("SELECT public.gps_tracking_user_requires_mfa($1::uuid)")
            .bind(&user.user_id)
            .fetch_one(&auth.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if required {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut tx = auth
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM mfa_factors WHERE user_id=$1::uuid")
        .bind(&user.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id=$1::uuid")
        .bind(&user.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE users SET auth_version=auth_version+1,updated_at=clock_timestamp() WHERE id=$1::uuid").bind(&user.user_id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE user_id=$1::uuid AND revoked_at IS NULL").bind(&user.user_id).execute(&mut *tx).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response.headers_mut().append(
        header::SET_COOKIE,
        "__Host-tracking_session=; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    Ok(response)
}

pub(super) async fn issue_session(
    pool: &PgPool,
    _headers: &HeaderMap,
    id: &str,
    email: &str,
    display_name: &str,
    created_at: &str,
    auth_version: i64,
) -> Result<axum::response::Response, StatusCode> {
    let secret = random_secret();
    let csrf = random_secret();
    let sid = uuid::Uuid::new_v4();
    sqlx::query("INSERT INTO auth_sessions(id,user_id,secret_hash,csrf_hash,auth_version,expires_at) VALUES($1::uuid,$2::uuid,$3,$4,$5,clock_timestamp()+interval '12 hours')")
        .bind(sid.to_string()).bind(id).bind(digest(&secret)).bind(digest(&csrf)).bind(auth_version).execute(pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    let updated_at: String =
        sqlx::query_scalar("SELECT updated_at::text FROM users WHERE id=$1::uuid")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let result = LoginResult::Authenticated {
        user: PublicUser {
            id: id.into(),
            email: email.into(),
            display_name: display_name.into(),
            status: "active".into(),
            created_at: created_at.into(),
            updated_at,
        },
    };
    let mut response = (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(result),
    )
        .into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        cookie("__Host-tracking_session", &secret, 43200)
            .parse()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        "__Host-tracking_pre_auth=; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
            .parse()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    Ok(response)
}

pub(super) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_body): Json<EmptyRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    if !same_origin(&headers, &auth.origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    let secret =
        cookie_value(&headers, "__Host-tracking_session").ok_or(StatusCode::UNAUTHORIZED)?;
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    let result=sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE secret_hash=$1 AND csrf_hash=$2 AND revoked_at IS NULL")
        .bind(digest(secret)).bind(digest(csrf)).execute(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        let session_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM auth_sessions WHERE secret_hash=$1)")
                .bind(digest(secret))
                .fetch_one(&auth.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if session_exists {
            let csrf_matches: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM auth_sessions WHERE secret_hash=$1 AND csrf_hash=$2)",
            )
            .bind(digest(secret))
            .bind(digest(csrf))
            .fetch_one(&auth.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if !csrf_matches {
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }
    let mut response = (
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        "__Host-tracking_session=; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    Ok(response)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyRequest {}

pub(super) async fn revoke_session(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let auth = state.auth.ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    if !same_origin(&headers, &auth.origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    if uuid::Uuid::parse_str(&session_id).is_err() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let secret =
        cookie_value(&headers, "__Host-tracking_session").ok_or(StatusCode::UNAUTHORIZED)?;
    let csrf = headers
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;
    let current=sqlx::query_as::<_,(String,String)>("SELECT s.id::text,s.user_id::text FROM auth_sessions s JOIN users u ON u.id=s.user_id WHERE s.secret_hash=$1 AND s.csrf_hash=$2 AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() AND s.last_interactive_at>clock_timestamp()-interval '30 minutes' AND s.auth_version=u.auth_version AND u.status='active'")
        .bind(digest(secret)).bind(digest(csrf)).fetch_optional(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?.ok_or(StatusCode::UNAUTHORIZED)?;
    let result=sqlx::query("UPDATE auth_sessions SET revoked_at=clock_timestamp() WHERE id=$1::uuid AND user_id=$2::uuid AND revoked_at IS NULL")
        .bind(session_id).bind(current.1).execute(&auth.pool).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    ))
}

pub(super) fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
pub(super) fn digest(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}
pub(super) fn cookie(name: &str, value: &str, max_age: u32) -> String {
    format!("{name}={value}; Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}")
}
pub(super) fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
}
pub(super) fn valid_pre_auth_csrf(headers: &HeaderMap) -> bool {
    let Some(cookie) = cookie_value(headers, "__Host-tracking_pre_auth") else {
        return false;
    };
    let Some(token) = headers.get("x-csrf-token").and_then(|v| v.to_str().ok()) else {
        return false;
    };
    constant_time_eq(cookie.as_bytes(), token.as_bytes())
}
pub(super) fn same_origin(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|value| constant_time_eq(value.as_bytes(), expected.as_bytes()))
}

/// Safe same-origin read-only GETs may omit `Origin` in browsers. Accept an
/// exact-origin `Referer` fallback here; state-changing auth routes continue
/// to require the `Origin` header through `same_origin`.
pub(super) fn same_origin_or_same_origin_referer(headers: &HeaderMap, expected: &str) -> bool {
    if same_origin(headers, expected) {
        return true;
    }
    let Some(referer) = headers
        .get(header::REFERER)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let (Some(referer_uri), Ok(expected_uri)) = (
        referer.parse::<axum::http::Uri>().ok(),
        expected.parse::<axum::http::Uri>(),
    ) else {
        return false;
    };
    matches!(referer_uri.scheme_str(), Some("http" | "https"))
        && referer_uri.scheme_str() == expected_uri.scheme_str()
        && referer_uri.authority() == expected_uri.authority()
}
pub(super) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && bool::from(a.ct_eq(b))
}
