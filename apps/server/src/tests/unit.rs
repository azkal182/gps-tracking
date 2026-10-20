use super::*;
use axum::{body::Body, http::Request};
use std::collections::HashSet;
use tower::ServiceExt;

#[tokio::test]
async fn health_and_readiness_endpoints_report_status() {
    let state = AppState::new();
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app(state.clone())
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    state.set_ready(false);
    let response = app(state)
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn realtime_outbound_queue_is_bounded_at_256_and_removes_slow_consumer() {
    let state = AppState::new();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(256);
    let subscriptions = Arc::new(Mutex::new(Some((
        HashSet::new(),
        HashSet::from(["device-a".to_owned()]),
    ))));
    state.realtime_clients.lock().unwrap().insert(
        "connection-a".into(),
        RealtimeConnection {
            tenant_id: "tenant-a".into(),
            subscriptions,
            sender,
        },
    );
    for index in 0..257 {
        publish_event(
            &state,
            "tenant-a",
            Some("device-a"),
            None,
            None,
            "device.connectivity_changed",
            "2026-10-19T00:00:00Z",
            serde_json::json!({"index":index}),
        );
    }
    assert!(
        !state
            .realtime_clients
            .lock()
            .unwrap()
            .contains_key("connection-a")
    );
    for _ in 0..256 {
        assert!(receiver.try_recv().is_ok());
    }
    assert!(receiver.try_recv().is_err());
}

#[test]
fn secret_debug_is_redacted() {
    let secret = Secret("postgres://private-user:secret@example/db".to_owned());
    assert_eq!(format!("{secret:?}"), "[REDACTED]");
    assert!(
        !format!(
            "{:?}",
            Config {
                bind: "127.0.0.1:8080".parse().unwrap(),
                database_url: Some(secret),
                public_origin: "http://localhost:5173".into(),
                position_freshness_seconds: 60,
                device_offline_timeout_seconds: 180,
                position_retention_days: 365,
            }
        )
        .contains("secret@example")
    );
}

#[test]
fn position_retention_days_defaults_to_365_and_rejects_out_of_range() {
    assert_eq!(position_retention_days_value(None), Ok(365));
    assert_eq!(position_retention_days_value(Some("60")), Ok(60));
    assert_eq!(position_retention_days_value(Some("1")), Ok(1));
    assert_eq!(
        position_retention_days_value(Some("36501")),
        Err(ConfigError::InvalidPositionRetentionDays)
    );
    assert_eq!(
        position_retention_days_value(Some("0")),
        Err(ConfigError::InvalidPositionRetentionDays)
    );
    assert_eq!(
        position_retention_days_value(Some("abc")),
        Err(ConfigError::InvalidPositionRetentionDays)
    );
}

#[test]
fn tracking_status_thresholds_default_and_require_positive_seconds() {
    assert_eq!(
        positive_seconds_value("POSITION_FRESHNESS_SECONDS", None, 60),
        Ok(60)
    );
    assert_eq!(
        positive_seconds_value("DEVICE_OFFLINE_TIMEOUT_SECONDS", Some("240".into()), 180),
        Ok(240)
    );
    assert_eq!(
        positive_seconds_value("DEVICE_OFFLINE_TIMEOUT_SECONDS", Some("0".into()), 180),
        Err(ConfigError::InvalidStatusThreshold(
            "DEVICE_OFFLINE_TIMEOUT_SECONDS"
        ))
    );
}

#[test]
fn forgot_password_rate_limits_recipient_and_ip_together() {
    let limiter = Mutex::new(HashMap::new());
    let ip = "ip:synthetic".to_owned();
    let recipient = "recipient:synthetic".to_owned();
    for _ in 0..3 {
        assert!(take_limited(
            &limiter,
            &[(recipient.clone(), 3), (ip.clone(), 20)]
        ));
    }
    assert!(!take_limited(
        &limiter,
        &[(recipient.clone(), 3), (ip.clone(), 20)]
    ));
    for index in 1..18 {
        assert!(take_limited(
            &limiter,
            &[(format!("recipient:{index}"), 3), (ip.clone(), 20)]
        ));
    }
    assert!(!take_limited(
        &limiter,
        &[("recipient:last".into(), 3), (ip.clone(), 20)]
    ));
}

#[test]
fn login_failure_buckets_are_account_scoped_and_expire_after_the_window() {
    let limiter = Mutex::new(HashMap::new());
    let account = "login:failure:synthetic".to_owned();
    for _ in 0..5 {
        assert!(take_limited_for(
            &limiter,
            &[(account.clone(), 5)],
            Duration::from_secs(15 * 60)
        ));
    }
    assert!(is_limited_for(
        &limiter,
        &account,
        5,
        Duration::from_secs(15 * 60)
    ));
    clear_limited_bucket(&limiter, &account);
    assert!(!is_limited_for(
        &limiter,
        &account,
        5,
        Duration::from_secs(15 * 60)
    ));
    limiter.lock().unwrap().insert(
        account.clone(),
        VecDeque::from([Instant::now() - Duration::from_secs(16 * 60)]),
    );
    assert!(!is_limited_for(
        &limiter,
        &account,
        5,
        Duration::from_secs(15 * 60)
    ));
}

#[test]
fn pre_auth_csrf_requires_matching_cookie_and_header_and_exact_origin() {
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, "https://portal.example".parse().unwrap());
    headers.insert(
        header::COOKIE,
        "__Host-tracking_pre_auth=abc123".parse().unwrap(),
    );
    headers.insert("x-csrf-token", "abc123".parse().unwrap());
    assert!(same_origin(&headers, "https://portal.example"));
    assert!(valid_pre_auth_csrf(&headers));
    headers.insert("x-csrf-token", "wrong".parse().unwrap());
    assert!(!valid_pre_auth_csrf(&headers));
    assert!(!same_origin(&headers, "https://evil.example"));
    assert!(!cookie("__Host-tracking_session", "synthetic", 43200).contains("Domain="));
}

#[test]
fn csrf_bootstrap_accepts_same_origin_referer_when_get_omits_origin() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::REFERER,
        "http://localhost:5173/login".parse().unwrap(),
    );
    assert!(same_origin_or_same_origin_referer(
        &headers,
        "http://localhost:5173"
    ));
    assert!(!same_origin(&headers, "http://localhost:5173"));

    headers.insert(
        header::REFERER,
        "http://localhost:51730/login".parse().unwrap(),
    );
    assert!(!same_origin_or_same_origin_referer(
        &headers,
        "http://localhost:5173"
    ));
    headers.insert(
        header::REFERER,
        "https://evil.example/login".parse().unwrap(),
    );
    assert!(!same_origin_or_same_origin_referer(
        &headers,
        "http://localhost:5173"
    ));
}

#[test]
fn session_and_challenge_cookies_have_required_flags_and_secrets_are_hashed() {
    let value = cookie("__Host-tracking_session", "synthetic", 43200);
    for flag in ["Secure", "HttpOnly", "SameSite=Lax", "Path=/"] {
        assert!(value.contains(flag));
    }
    assert_eq!(digest("synthetic").len(), 32);
    assert_ne!(digest("synthetic"), b"synthetic");
}

#[test]
fn new_password_hashes_use_argon2id_project_parameters_and_unique_salts() {
    let first = hash_password(b"synthetic password for auth tests").unwrap();
    let second = hash_password(b"synthetic password for auth tests").unwrap();
    assert!(first.starts_with("$argon2id$v=19$m=65536,t=3,p=1$"));
    assert_ne!(first, second);
    assert!(!first.contains("synthetic password"));
}
