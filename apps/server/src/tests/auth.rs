use super::*;
use axum::http::Request;
use tower::ServiceExt;

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
async fn auth_login_csrf_timeout_logout_and_revocation_lifecycle() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let pool = PgPool::connect(&std::env::var("GPS_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let user_id = uuid::Uuid::new_v4().to_string();
    let email = format!("auth-{}@example.invalid", &user_id[..8]);
    let password = "synthetic test password 123";
    let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
    use argon2::PasswordHasher;
    let encoded = argon2::Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(19 * 1024, 2, 1, None).unwrap(),
    )
    .hash_password(password.as_bytes(), &salt)
    .unwrap()
    .to_string();
    sqlx::query("INSERT INTO users(id,email_canonical,display_name,password_hash) VALUES($1::uuid,$2,'Synthetic User',$3)")
            .bind(&user_id).bind(&email).bind(&encoded).execute(&pool).await.unwrap();
    let organization_id = uuid::Uuid::new_v4().to_string();
    let membership_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'Synthetic auth test org')")
        .bind(&organization_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
            .bind(&membership_id).bind(&organization_id).bind(&user_id).execute(&pool).await.unwrap();
    let mut auth_connection = pool.acquire().await.unwrap();
    sqlx::query("SET ROLE gps_tracking_auth")
        .execute(&mut *auth_connection)
        .await
        .unwrap();
    let required: bool = sqlx::query_scalar("SELECT gps_tracking_user_requires_mfa($1::uuid)")
        .bind(&user_id)
        .fetch_one(&mut *auth_connection)
        .await
        .unwrap();
    assert!(
        required,
        "mandatory MFA policy must be queryable before tenant context"
    );
    let can_read_positions = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM positions")
        .fetch_one(&mut *auth_connection)
        .await;
    assert!(
        can_read_positions.is_err(),
        "auth capability cannot read location tables"
    );
    sqlx::query("RESET ROLE")
        .execute(&mut *auth_connection)
        .await
        .unwrap();
    drop(auth_connection);
    sqlx::query("UPDATE memberships SET role='viewer' WHERE id=$1::uuid")
        .bind(&membership_id)
        .execute(&pool)
        .await
        .unwrap();
    let application = app(AppState::with_auth_test_key(
        pool.clone(),
        "https://portal.example".into(),
        [9; 32],
    ));
    let csrf_response = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/csrf")
                .header("origin", "https://portal.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(csrf_response.status(), StatusCode::OK);
    let pre_cookie = csrf_response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf: serde_json::Value = serde_json::from_slice(
        &to_bytes(csrf_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let csrf = csrf["csrf_token"].as_str().unwrap();
    let no_csrf = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &pre_cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(no_csrf.status(), StatusCode::FORBIDDEN);
    assert_eq!(no_csrf.headers()[header::CACHE_CONTROL], "no-store");
    let wrong = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &pre_cookie)
                .header("x-csrf-token", csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":"missing@example.invalid","password":password})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    let wrong_password = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &pre_cookie)
                .header("x-csrf-token", csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":"incorrect synthetic password"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(wrong_password.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        to_bytes(wrong.into_body(), usize::MAX).await.unwrap(),
        to_bytes(wrong_password.into_body(), usize::MAX)
            .await
            .unwrap(),
        "unknown principal and wrong password have identical public errors"
    );
    let login = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &pre_cookie)
                .header("x-csrf-token", csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    assert_eq!(login.headers()[header::CACHE_CONTROL], "no-store");
    let set_cookie = login
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|h| h.to_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(set_cookie.contains("__Host-tracking_session="));
    assert!(set_cookie.contains("Secure; HttpOnly; SameSite=Lax; Path=/"));
    let session_secret = set_cookie
        .lines()
        .find_map(|line| line.strip_prefix("__Host-tracking_session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let login_body = String::from_utf8(
        to_bytes(login.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(!login_body.contains(password));
    assert!(!login_body.contains(&session_secret));
    let stored: (Vec<u8>,) =
        sqlx::query_as("SELECT secret_hash FROM auth_sessions WHERE secret_hash=$1")
            .bind(digest(&session_secret))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0.len(), 32);
    assert_ne!(stored.0, session_secret.as_bytes());
    let session_id: String =
        sqlx::query_scalar("SELECT id::text FROM auth_sessions WHERE secret_hash=$1")
            .bind(digest(&session_secret))
            .fetch_one(&pool)
            .await
            .unwrap();
    let same_origin_read = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/organizations?limit=100")
                .header(header::REFERER, "https://portal.example/overview")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        same_origin_read.status(),
        StatusCode::OK,
        "read-only same-origin GET accepts exact Referer when Origin is omitted"
    );
    let cross_origin_read = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/organizations?limit=100")
                .header(header::REFERER, "https://attacker.example/overview")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cross_origin_read.status(), StatusCode::FORBIDDEN);
    let session_page = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/sessions?limit=100")
                .header(header::REFERER, "https://portal.example/sessions")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(session_page.status(), StatusCode::OK);
    let session_page: serde_json::Value = serde_json::from_slice(
        &to_bytes(session_page.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(session_page["data"].as_array().unwrap().iter().any(|row| {
        row["id"] == session_id
            && row["current"] == true
            && row["device_label"] == "Browser session"
    }));
    let absolute_window: bool=sqlx::query_scalar("SELECT expires_at<=created_at+interval '12 hours' AND expires_at>clock_timestamp() FROM auth_sessions WHERE id=$1::uuid")
            .bind(&session_id).fetch_one(&pool).await.unwrap();
    assert!(absolute_window);
    let mut session_headers = HeaderMap::new();
    session_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={session_secret}")
            .parse()
            .unwrap(),
    );
    assert_eq!(
        authenticate_cookie_session(&pool, &session_headers, true)
            .await
            .unwrap()
            .user_id,
        user_id
    );
    let csrf_response = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/csrf")
                .header("origin", "https://portal.example")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let refreshed_pre_auth_cookie = csrf_response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            let value = value.to_str().ok()?;
            value
                .starts_with("__Host-tracking_pre_auth=")
                .then(|| value.split(';').next().unwrap().to_owned())
        })
        .expect("session CSRF refresh also synchronizes the pre-auth cookie");
    let auth_csrf: serde_json::Value = serde_json::from_slice(
        &to_bytes(csrf_response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let auth_csrf = auth_csrf["csrf_token"].as_str().unwrap().to_owned();
    assert_eq!(
        refreshed_pre_auth_cookie
            .strip_prefix("__Host-tracking_pre_auth=")
            .unwrap(),
        auth_csrf
    );
    let re_login = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &refreshed_pre_auth_cookie)
                .header("x-csrf-token", &auth_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(re_login.status(), StatusCode::OK);
    sqlx::query("UPDATE auth_sessions SET created_at=clock_timestamp()-interval '13 hours', expires_at=clock_timestamp()-interval '1 second' WHERE id=$1::uuid")
            .bind(&session_id).execute(&pool).await.unwrap();
    assert_eq!(
        authenticate_cookie_session(&pool, &session_headers, true)
            .await
            .unwrap_err(),
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE auth_sessions SET created_at=clock_timestamp()-interval '31 minutes', expires_at=clock_timestamp()+interval '11 hours' WHERE id=$1::uuid")
            .bind(&session_id).execute(&pool).await.unwrap();
    let revoke_id = uuid::Uuid::new_v4().to_string();
    let revoke_secret = random_secret();
    let revoke_csrf = random_secret();
    sqlx::query("INSERT INTO auth_sessions(id,user_id,secret_hash,csrf_hash,auth_version,expires_at) VALUES($1::uuid,$2::uuid,$3,$4,0,clock_timestamp()+interval '12 hours')")
            .bind(&revoke_id).bind(&user_id).bind(digest(&revoke_secret)).bind(digest(&revoke_csrf)).execute(&pool).await.unwrap();
    let revoked = application
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/auth/sessions/{revoke_id}"))
                .header("origin", "https://portal.example")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .header("x-csrf-token", &auth_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let revoked_at: Option<String> =
        sqlx::query_scalar("SELECT revoked_at::text FROM auth_sessions WHERE id=$1::uuid")
            .bind(&revoke_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(revoked_at.is_some());
    sqlx::query("UPDATE auth_sessions SET created_at=clock_timestamp()-interval '31 minutes', last_interactive_at=clock_timestamp()-interval '31 minutes', expires_at=clock_timestamp()+interval '11 hours' WHERE id=$1::uuid").bind(&session_id).execute(&pool).await.unwrap();
    let expired = application
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/auth/sessions/{session_id}"))
                .header("origin", "https://portal.example")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .header("x-csrf-token", &auth_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);
    let stale_csrf = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/csrf")
                .header("origin", "https://portal.example")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale_csrf.status(), StatusCode::OK);
    let stale_cookie_headers = stale_csrf.headers().get_all(header::SET_COOKIE);
    assert!(stale_cookie_headers.iter().any(|value| {
        value.to_str().is_ok_and(|cookie| {
            cookie.starts_with("__Host-tracking_session=") && cookie.contains("Max-Age=0")
        })
    }));
    assert!(stale_cookie_headers.iter().any(|value| {
        value
            .to_str()
            .is_ok_and(|cookie| cookie.starts_with("__Host-tracking_pre_auth="))
    }));
    let stale_csrf_body: serde_json::Value =
        serde_json::from_slice(&to_bytes(stale_csrf.into_body(), usize::MAX).await.unwrap())
            .unwrap();
    assert!(
        stale_csrf_body["csrf_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    );
    assert_eq!(
        authenticate_cookie_session(&pool, &session_headers, true)
            .await
            .unwrap_err(),
        StatusCode::UNAUTHORIZED
    );
    let logout = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/logout")
                .header("origin", "https://portal.example")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .header("x-csrf-token", &auth_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    let repeated_logout = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/logout")
                .header("origin", "https://portal.example")
                .header(
                    header::COOKIE,
                    format!("__Host-tracking_session={session_secret}"),
                )
                .header("x-csrf-token", &auth_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repeated_logout.status(), StatusCode::NO_CONTENT);
    let logout_revoked: Option<String> =
        sqlx::query_scalar("SELECT revoked_at::text FROM auth_sessions WHERE id=$1::uuid")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(logout_revoked.is_some());
    sqlx::query("UPDATE memberships SET role='organization_admin' WHERE id=$1::uuid")
        .bind(&membership_id)
        .execute(&pool)
        .await
        .unwrap();
    let challenge_csrf = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/csrf")
                .header("origin", "https://portal.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let challenge_cookie = challenge_csrf.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let challenge_token: serde_json::Value = serde_json::from_slice(
        &to_bytes(challenge_csrf.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let challenge_login = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, challenge_cookie)
                .header(
                    "x-csrf-token",
                    challenge_token["csrf_token"].as_str().unwrap(),
                )
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(challenge_login.status(), StatusCode::OK);
    let challenge_set_cookie = challenge_login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap();
    assert!(challenge_set_cookie.contains("__Host-tracking_challenge="));
    assert!(challenge_set_cookie.contains("Secure; HttpOnly; SameSite=Lax; Path=/; Max-Age=300"));
    let challenge_secret = challenge_set_cookie
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_owned();
    let mut fake_session_headers = HeaderMap::new();
    fake_session_headers.insert(
        header::COOKIE,
        format!("__Host-tracking_session={challenge_secret}")
            .parse()
            .unwrap(),
    );
    assert_eq!(
        authenticate_cookie_session(&pool, &fake_session_headers, false)
            .await
            .unwrap_err(),
        StatusCode::UNAUTHORIZED
    );
    let challenge_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(challenge_login.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(challenge_body["state"], "mfa_enrollment_required");
    assert!(!challenge_body.to_string().contains(&challenge_secret));
    let stored_challenge:(i16,bool,bool)=sqlx::query_as("SELECT attempt_count, expires_at<=created_at+interval '5 minutes', expires_at>clock_timestamp() FROM auth_challenges WHERE user_id=$1::uuid ORDER BY created_at DESC LIMIT 1").bind(&user_id).fetch_one(&pool).await.unwrap();
    assert_eq!(stored_challenge.0, 0);
    assert!(stored_challenge.1);
    assert!(stored_challenge.2);
    let over_limit =
        sqlx::query("UPDATE auth_challenges SET attempt_count=6 WHERE user_id=$1::uuid")
            .bind(&user_id)
            .execute(&pool)
            .await;
    assert!(
        over_limit.is_err(),
        "storage rejects challenge attempts above five"
    );
    sqlx::query("DELETE FROM memberships WHERE id=$1::uuid")
        .bind(membership_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM organizations WHERE id=$1::uuid")
        .bind(organization_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id=$1::uuid")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
async fn mfa_login_recovery_and_reenrollment_are_atomic_and_restricted() {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let pool = PgPool::connect(&std::env::var("GPS_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let key = [41u8; 32];
    let user_id = uuid::Uuid::new_v4().to_string();
    let email = format!("mfa-{}@example.invalid", &user_id[..8]);
    let password = "synthetic mfa password 123";
    let password_hash = hash_password(password.as_bytes()).unwrap();
    sqlx::query("INSERT INTO users(id,email_canonical,display_name,password_hash,platform_admin) VALUES($1::uuid,$2,'Synthetic MFA',$3,TRUE)").bind(&user_id).bind(&email).bind(password_hash).execute(&pool).await.unwrap();
    let org = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,'MFA test')")
        .bind(&org)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')").bind(uuid::Uuid::new_v4().to_string()).bind(&org).bind(&user_id).execute(&pool).await.unwrap();
    let raw = mfa::new_secret();
    let (ciphertext, nonce) = mfa::encrypt(&key, &raw).unwrap();
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())").bind(uuid::Uuid::new_v4().to_string()).bind(&user_id).bind(ciphertext).bind(nonce).execute(&pool).await.unwrap();
    let app = app(AppState::with_auth_test_key(
        pool.clone(),
        "https://portal.example".into(),
        key,
    ));
    let csrf_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/csrf")
                .header("origin", "https://portal.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let pre_cookie = csrf_resp.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf: serde_json::Value =
        serde_json::from_slice(&to_bytes(csrf_resp.into_body(), usize::MAX).await.unwrap())
            .unwrap();
    let csrf = csrf["csrf_token"].as_str().unwrap().to_owned();
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &pre_cookie)
                .header("x-csrf-token", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    let mut challenge_cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let mut challenge_as_session = HeaderMap::new();
    challenge_as_session.insert(header::COOKIE, challenge_cookie.parse().unwrap());
    assert_eq!(
        authenticate_cookie_session(&pool, &challenge_as_session, false)
            .await
            .unwrap_err(),
        StatusCode::UNAUTHORIZED,
        "MFA challenge is not an application session"
    );
    for _ in 0..5 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/auth/mfa/verify")
                    .header("origin", "https://portal.example")
                    .header(header::COOKIE, format!("{pre_cookie}; {challenge_cookie}"))
                    .header("x-csrf-token", &csrf)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"code":"abcdef"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let attempts: (i16, bool) = sqlx::query_as(
        "SELECT attempt_count,consumed_at IS NOT NULL FROM auth_challenges WHERE secret_hash=$1",
    )
    .bind(digest(challenge_cookie.split_once('=').unwrap().1))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        attempts,
        (5, true),
        "challenge is atomically invalidated after five failures"
    );
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &pre_cookie)
                .header("x-csrf-token", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    challenge_cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let current = current_totp_step().unwrap();
    let code = mfa::totp(&raw, current);
    let make_verify = || {
        Request::builder()
            .method("POST")
            .uri("/v1/auth/mfa/verify")
            .header("origin", "https://portal.example")
            .header(header::COOKIE, format!("{pre_cookie}; {challenge_cookie}"))
            .header("x-csrf-token", &csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::json!({"code":code}).to_string()))
            .unwrap()
    };
    let (one, two) = tokio::join!(
        app.clone().oneshot(make_verify()),
        app.clone().oneshot(make_verify())
    );
    let responses = [one.unwrap(), two.unwrap()];
    let mut statuses = [responses[0].status(), responses[1].status()];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::UNAUTHORIZED],
        "concurrent TOTP step can authenticate exactly once"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM auth_sessions WHERE user_id=$1::uuid AND revoked_at IS NULL"
        )
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    let login_session = responses
        .iter()
        .find(|response| response.status() == StatusCode::OK)
        .unwrap()
        .headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let me = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/me")
                .header(header::REFERER, "https://portal.example/login")
                .header(header::COOKIE, &login_session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let me_status = me.status();
    let me_body = to_bytes(me.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        me_status,
        StatusCode::OK,
        "auth/me response: {}",
        String::from_utf8_lossy(&me_body)
    );
    let me: serde_json::Value = serde_json::from_slice(&me_body).unwrap();
    assert_eq!(me["user"]["id"], user_id);
    assert_eq!(me["platform_admin"], true);
    assert_eq!(me["mfa_required"], true);
    assert_eq!(me["mfa_enabled"], true);
    assert_eq!(me["memberships"][0]["tenant_id"], org);
    assert_eq!(me["memberships"][0]["role"], "admin");

    let csrf_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/auth/csrf")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &login_session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let session_csrf: serde_json::Value =
        serde_json::from_slice(&to_bytes(csrf_resp.into_body(), usize::MAX).await.unwrap())
            .unwrap();
    let session_csrf = session_csrf["csrf_token"].as_str().unwrap().to_owned();
    let reauth_request = |password_value: &str, code: Option<String>| {
        Request::builder()
            .method("POST")
            .uri("/v1/auth/reauthenticate")
            .header("origin", "https://portal.example")
            .header(header::COOKIE, &login_session)
            .header("x-csrf-token", &session_csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"password":password_value,"code":code}).to_string(),
            ))
            .unwrap()
    };
    let missing_code = app
        .clone()
        .oneshot(reauth_request(password, None))
        .await
        .unwrap();
    assert_eq!(
        missing_code.status(),
        StatusCode::FORBIDDEN,
        "password-only reauthentication cannot bypass MFA"
    );
    let wrong_password = app
        .clone()
        .oneshot(reauth_request(
            "incorrect synthetic password",
            Some(mfa::totp(&raw, current + 1)),
        ))
        .await
        .unwrap();
    assert_eq!(wrong_password.status(), StatusCode::UNAUTHORIZED);
    let recent = app
        .clone()
        .oneshot(reauth_request(password, Some(mfa::totp(&raw, current + 1))))
        .await
        .unwrap();
    assert_eq!(recent.status(), StatusCode::NO_CONTENT);
    assert!(sqlx::query_scalar::<_,bool>("SELECT reauthenticated_at>clock_timestamp()-interval '5 minutes' FROM auth_sessions WHERE secret_hash=$1").bind(digest(login_session.split_once('=').unwrap().1)).fetch_one(&pool).await.unwrap());
    let rotate = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/mfa/recovery-codes/rotate")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &login_session)
                .header("x-csrf-token", &session_csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate.status(), StatusCode::OK);
    let rotated: serde_json::Value =
        serde_json::from_slice(&to_bytes(rotate.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(rotated["recovery_codes"].as_array().unwrap().len(), 10);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM recovery_codes WHERE user_id=$1::uuid AND consumed_at IS NULL"
        )
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        10,
        "rotation replaces all previous available codes"
    );
    let disable = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/auth/mfa")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &login_session)
                .header("x-csrf-token", &session_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        disable.status(),
        StatusCode::FORBIDDEN,
        "mandatory MFA cannot be disabled"
    );

    let recovery = random_secret();
    sqlx::query("INSERT INTO recovery_codes(id,user_id,code_hash) VALUES($1::uuid,$2::uuid,$3)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&user_id)
        .bind(digest(&recovery))
        .execute(&pool)
        .await
        .unwrap();
    let login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &pre_cookie)
                .header("x-csrf-token", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let challenge_cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let make_recover = || {
        Request::builder()
            .method("POST")
            .uri("/v1/auth/mfa/recover")
            .header("origin", "https://portal.example")
            .header(header::COOKIE, format!("{pre_cookie}; {challenge_cookie}"))
            .header("x-csrf-token", &csrf)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({"recovery_code":recovery}).to_string(),
            ))
            .unwrap()
    };
    let (one, two) = tokio::join!(
        app.clone().oneshot(make_recover()),
        app.clone().oneshot(make_recover())
    );
    let results = [one.unwrap(), two.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|r| r.status() == StatusCode::OK)
            .count(),
        1,
        "recovery code can be consumed once under concurrency"
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| r.status() == StatusCode::UNAUTHORIZED)
            .count(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM auth_sessions WHERE user_id=$1::uuid AND revoked_at IS NULL"
        )
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "recovery revokes previous sessions"
    );
    let success = results
        .iter()
        .find(|r| r.status() == StatusCode::OK)
        .unwrap();
    let restricted_cookie = success.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let begin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/mfa/enrollment")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, format!("{pre_cookie}; {restricted_cookie}"))
                .header("x-csrf-token", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(begin.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(begin.into_body(), usize::MAX).await.unwrap()).unwrap();
    let new_raw = mfa::base32_decode(body["secret"].as_str().unwrap()).unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL"
        )
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
        "old active factor remains until confirmation"
    );
    let confirm = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/mfa/enrollment/confirm")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, format!("{pre_cookie}; {restricted_cookie}"))
                .header("x-csrf-token", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"code":mfa::totp(&new_raw,current_totp_step().unwrap())})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(confirm.status(), StatusCode::OK);
    let recovery_body: serde_json::Value =
        serde_json::from_slice(&to_bytes(confirm.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
        recovery_body["recovery_codes"].as_array().unwrap().len(),
        10
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL"
        )
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM recovery_codes WHERE user_id=$1::uuid AND consumed_at IS NULL"
        )
        .bind(&user_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        10,
        "re-enrollment rotates recovery codes"
    );
    sqlx::query("DELETE FROM mfa_factors WHERE user_id=$1::uuid")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE memberships SET status='pending_mfa' WHERE user_id=$1::uuid")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    let pending_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/login")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, &pre_cookie)
                .header("x-csrf-token", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"email":email,"password":password}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pending_login.status(), StatusCode::OK);
    let pending_cookie = pending_login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let pending_body: serde_json::Value = serde_json::from_slice(
        &to_bytes(pending_login.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(pending_body["state"], "mfa_enrollment_required");
    let pending_begin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/auth/mfa/enrollment")
                .header("origin", "https://portal.example")
                .header(header::COOKIE, format!("{pre_cookie}; {pending_cookie}"))
                .header("x-csrf-token", &csrf)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pending_begin.status(), StatusCode::OK);
    let pending_enrollment: serde_json::Value = serde_json::from_slice(
        &to_bytes(pending_begin.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let pending_secret =
        mfa::base32_decode(pending_enrollment["secret"].as_str().unwrap()).unwrap();
    let pending_confirm=app.clone().oneshot(Request::builder().method("POST").uri("/v1/auth/mfa/enrollment/confirm").header("origin","https://portal.example").header(header::COOKIE,format!("{pre_cookie}; {pending_cookie}")).header("x-csrf-token",&csrf).header(header::CONTENT_TYPE,"application/json").body(Body::from(serde_json::json!({"code":mfa::totp(&pending_secret,current_totp_step().unwrap())}).to_string())).unwrap()).await.unwrap();
    assert_eq!(pending_confirm.status(), StatusCode::OK);
    assert!(
        pending_confirm
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .any(|value| value
                .to_str()
                .unwrap()
                .starts_with("__Host-tracking_session=")),
        "pending_mfa confirmation issues a full session"
    );
    let membership_status: String = sqlx::query_scalar(
        "SELECT status FROM memberships WHERE user_id=$1::uuid AND tenant_id=$2::uuid",
    )
    .bind(&user_id)
    .bind(&org)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(membership_status, "active");
    sqlx::query("DELETE FROM memberships WHERE tenant_id=$1::uuid")
        .bind(&org)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM organizations WHERE id=$1::uuid")
        .bind(org)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM users WHERE id=$1::uuid")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires GPS_TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
async fn auth03_reset_activation_and_operator_recovery_invariants() {
    let pool = PgPool::connect(&std::env::var("GPS_TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    tracking_storage::migrate(&pool).await.unwrap();
    let mut state =
        AppState::with_auth_test_key(pool.clone(), "https://portal.example".into(), [51; 32]);
    if let Some(auth) = state.auth.as_mut() {
        auth.outbox_key = Some([52; 32]);
    }
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert(
        header::COOKIE,
        "__Host-tracking_pre_auth=csrf-fixture".parse().unwrap(),
    );
    headers.insert("x-csrf-token", "csrf-fixture".parse().unwrap());

    let reset_user = insert_auth03_user(&pool, "active").await;
    let reset_token = random_secret();
    let reset_hash = hash_password(b"old reset password fixture").unwrap();
    sqlx::query("UPDATE users SET password_hash=$1 WHERE id=$2::uuid")
        .bind(reset_hash)
        .bind(&reset_user)
        .execute(&pool)
        .await
        .unwrap();
    let factor_secret = mfa::new_secret();
    let (factor_cipher, factor_nonce) = mfa::encrypt(&[51; 32], &factor_secret).unwrap();
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&reset_user).bind(factor_cipher).bind(factor_nonce).execute(&pool).await.unwrap();
    let session_id = uuid::Uuid::new_v4().to_string();
    let session_secret = random_secret();
    let csrf_secret = random_secret();
    sqlx::query("INSERT INTO auth_sessions(id,user_id,secret_hash,csrf_hash,auth_version,expires_at) VALUES($1::uuid,$2::uuid,$3,$4,0,clock_timestamp()+interval '12 hours')")
            .bind(&session_id).bind(&reset_user).bind(digest(&session_secret)).bind(digest(&csrf_secret)).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'password_reset',$3,clock_timestamp()+interval '30 minutes')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&reset_user).bind(digest(&reset_token)).execute(&pool).await.unwrap();
    let scanner = app(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/auth/password/reset")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        scanner.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "GET scanner cannot consume token"
    );
    let result = reset_password(
        State(state.clone()),
        headers.clone(),
        Json(PasswordResetRequest {
            token: reset_token.clone(),
            new_password: "a valid replacement password".into(),
        }),
    )
    .await
    .unwrap()
    .into_response();
    assert_eq!(result.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM mfa_factors WHERE user_id=$1::uuid AND enabled_at IS NOT NULL"
        )
        .bind(&reset_user)
        .fetch_one(&pool)
        .await
        .unwrap(),
        1,
        "reset preserves MFA"
    );
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT revoked_at IS NOT NULL FROM auth_sessions WHERE id=$1::uuid"
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT consumed_at IS NOT NULL FROM account_tokens WHERE secret_hash=$1"
        )
        .bind(digest(&reset_token))
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    let replay = reset_password(
        State(state.clone()),
        headers.clone(),
        Json(PasswordResetRequest {
            token: reset_token,
            new_password: "a second replacement password".into(),
        }),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(replay, StatusCode::UNAUTHORIZED, "replay is rejected");

    let activation_user = insert_auth03_user(&pool, "pending_activation").await;
    let activation_token = random_secret();
    sqlx::query("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'activation',$3,clock_timestamp()+interval '72 hours')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&activation_user).bind(digest(&activation_token)).execute(&pool).await.unwrap();
    let activation_scanner = app(state.clone())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/auth/activate")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        activation_scanner.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "activation GET scanner cannot consume token"
    );
    let activation = activate_account(
        State(state.clone()),
        headers.clone(),
        Json(PasswordResetRequest {
            token: activation_token,
            new_password: "initial account password".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(activation.into_response().status(), StatusCode::NO_CONTENT);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM users WHERE id=$1::uuid")
            .bind(&activation_user)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "active"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM auth_sessions WHERE user_id=$1::uuid")
            .bind(&activation_user)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
        "activation does not auto-login"
    );

    let recovery_user = insert_auth03_user(&pool, "active").await;
    let (ciphertext, nonce) = mfa::encrypt(&[51; 32], &mfa::new_secret()).unwrap();
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES($1::uuid,$2::uuid,$3,$4,1,clock_timestamp())")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&recovery_user).bind(ciphertext).bind(nonce).execute(&pool).await.unwrap();
    let auth_token = random_secret();
    let proof_token = random_secret();
    sqlx::query("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'recovery',$3,clock_timestamp()+interval '15 minutes'),($4::uuid,$2::uuid,'recovery_email',$5,clock_timestamp()+interval '15 minutes')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&recovery_user).bind(digest(&auth_token)).bind(uuid::Uuid::new_v4().to_string()).bind(digest(&proof_token)).execute(&pool).await.unwrap();
    let recovery = complete_operator_recovery(
        State(state.clone()),
        headers.clone(),
        Json(RecoveryCompleteRequest {
            token: auth_token.clone(),
            email_token: proof_token.clone(),
            password: "recovery replacement password".into(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(recovery.status(), StatusCode::OK);
    assert!(
        recovery
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .any(|cookie| cookie
                .to_str()
                .unwrap()
                .starts_with("__Host-tracking_challenge="))
    );
    assert!(
        !recovery
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .any(|cookie| cookie
                .to_str()
                .unwrap()
                .starts_with("__Host-tracking_session="))
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM mfa_factors WHERE user_id=$1::uuid")
            .bind(&recovery_user)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT purpose FROM auth_challenges WHERE user_id=$1::uuid ORDER BY created_at DESC LIMIT 1").bind(&recovery_user).fetch_one(&pool).await.unwrap(), "mfa_recovery");

    let expired_user = insert_auth03_user(&pool, "active").await;
    let replay_recovery = complete_operator_recovery(
        State(state.clone()),
        headers.clone(),
        Json(RecoveryCompleteRequest {
            token: auth_token.clone(),
            email_token: proof_token.clone(),
            password: "replayed recovery password".into(),
        }),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        replay_recovery,
        StatusCode::UNAUTHORIZED,
        "recovery tokens are one-use"
    );
    let mismatched_auth = random_secret();
    let mismatched_proof = random_secret();
    sqlx::query("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'recovery',$3,clock_timestamp()+interval '15 minutes'),($4::uuid,$5::uuid,'recovery_email',$6,clock_timestamp()+interval '15 minutes')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&recovery_user).bind(digest(&mismatched_auth)).bind(uuid::Uuid::new_v4().to_string()).bind(&expired_user).bind(digest(&mismatched_proof)).execute(&pool).await.unwrap();
    let mismatched = complete_operator_recovery(
        State(state.clone()),
        headers.clone(),
        Json(RecoveryCompleteRequest {
            token: mismatched_auth.clone(),
            email_token: mismatched_proof.clone(),
            password: "mismatched account password".into(),
        }),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        mismatched,
        StatusCode::UNAUTHORIZED,
        "authorization and email proof must identify the same account"
    );
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM account_tokens WHERE secret_hash IN ($1,$2) AND consumed_at IS NULL").bind(digest(&mismatched_auth)).bind(digest(&mismatched_proof)).fetch_one(&pool).await.unwrap(), 2, "mismatched recovery rolls token consumption back");
    let expired = random_secret();
    sqlx::query("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,created_at,expires_at) VALUES($1::uuid,$2::uuid,'password_reset',$3,clock_timestamp()-interval '2 seconds',clock_timestamp()-interval '1 second')")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&expired_user).bind(digest(&expired)).execute(&pool).await.unwrap();
    let expired_response = reset_password(
        State(state.clone()),
        headers,
        Json(PasswordResetRequest {
            token: expired,
            new_password: "valid password after expiry".into(),
        }),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        expired_response,
        StatusCode::UNAUTHORIZED,
        "expired links are rejected"
    );
    pool.close().await;
}

async fn insert_auth03_user(pool: &PgPool, status: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let email = format!("auth03-{}@example.invalid", &id[..8]);
    sqlx::query("INSERT INTO users(id,email_canonical,display_name,password_hash,status) VALUES($1::uuid,$2,'AUTH-03 Fixture',$3,$4)")
            .bind(&id).bind(email).bind("$argon2id$v=19$m=65536,t=3,p=1$synthetic$synthetic-fixture-only").bind(status).execute(pool).await.unwrap();
    id
}
