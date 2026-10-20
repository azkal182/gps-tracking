use crate::{AppState, take_limited_for};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, Method, Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const ROUTES: [&str; 10] = [
    "health",
    "metrics",
    "auth",
    "positions_ingest",
    "heartbeat",
    "history",
    "realtime",
    "management",
    "api_other",
    "other",
];

#[derive(Default)]
pub(crate) struct OperationalMetrics {
    requests: [AtomicU64; 10],
    errors: [AtomicU64; 10],
    duration_micros: [AtomicU64; 10],
    auth_failures: AtomicU64,
    rate_limited: AtomicU64,
    positions_stored: AtomicU64,
    positions_duplicate: AtomicU64,
    positions_rejected: AtomicU64,
    ws_queue_overflow: AtomicU64,
}

impl OperationalMetrics {
    fn record(&self, route: usize, status: StatusCode, micros: u64) {
        self.requests[route].fetch_add(1, Ordering::Relaxed);
        self.duration_micros[route].fetch_add(micros, Ordering::Relaxed);
        if status.is_client_error() || status.is_server_error() {
            self.errors[route].fetch_add(1, Ordering::Relaxed);
        }
        if route == 2 && matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            self.auth_failures.fetch_add(1, Ordering::Relaxed);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            self.rate_limited.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn render(&self, state: &AppState) -> String {
        let mut output = String::from(
            "# HELP gps_http_requests_total Requests by fixed route class.\n# TYPE gps_http_requests_total counter\n",
        );
        for (index, route) in ROUTES.iter().enumerate() {
            output.push_str(&format!(
                "gps_http_requests_total{{route=\"{route}\"}} {}\n",
                self.requests[index].load(Ordering::Relaxed)
            ));
            output.push_str(&format!(
                "gps_http_errors_total{{route=\"{route}\"}} {}\n",
                self.errors[index].load(Ordering::Relaxed)
            ));
            output.push_str(&format!(
                "gps_http_request_duration_seconds_sum{{route=\"{route}\"}} {:.6}\n",
                self.duration_micros[index].load(Ordering::Relaxed) as f64 / 1_000_000.0
            ));
        }
        output.push_str(&format!(
            "gps_auth_failures_total {}\ngps_rate_limited_total {}\ngps_positions_stored_total {}\ngps_positions_duplicate_total {}\ngps_positions_rejected_total {}\ngps_ws_queue_overflow_total {}\n",
            self.auth_failures.load(Ordering::Relaxed),
            self.rate_limited.load(Ordering::Relaxed),
            self.positions_stored.load(Ordering::Relaxed),
            self.positions_duplicate.load(Ordering::Relaxed),
            self.positions_rejected.load(Ordering::Relaxed),
            self.ws_queue_overflow.load(Ordering::Relaxed)
        ));
        pool_metrics(
            &mut output,
            "auth",
            state.auth.as_ref().map(|auth| &auth.pool),
        );
        pool_metrics(&mut output, "tracking", state.tracking_pool.as_ref());
        pool_metrics(
            &mut output,
            "platform",
            state
                .auth
                .as_ref()
                .and_then(|auth| auth.platform_pool.as_ref()),
        );
        let connections = state
            .realtime_connections
            .lock()
            .map(|counts| counts.values().sum::<usize>())
            .unwrap_or_default();
        output.push_str(&format!("gps_ws_connections_active {connections}\n"));
        output
    }

    pub(crate) fn record_position_results(&self, stored: usize, duplicate: usize, rejected: usize) {
        self.positions_stored
            .fetch_add(stored as u64, Ordering::Relaxed);
        self.positions_duplicate
            .fetch_add(duplicate as u64, Ordering::Relaxed);
        self.positions_rejected
            .fetch_add(rejected as u64, Ordering::Relaxed);
    }

    pub(crate) fn record_ws_queue_overflow(&self) {
        self.ws_queue_overflow.fetch_add(1, Ordering::Relaxed);
    }
}

fn pool_metrics(output: &mut String, name: &str, pool: Option<&sqlx::PgPool>) {
    let (size, idle) = pool.map_or((0, 0), |pool| (pool.size(), pool.num_idle()));
    output.push_str(&format!(
        "gps_db_pool_connections{{pool=\"{name}\",state=\"open\"}} {size}\n"
    ));
    output.push_str(&format!(
        "gps_db_pool_connections{{pool=\"{name}\",state=\"idle\"}} {idle}\n"
    ));
}

fn route_class(method: &Method, path: &str) -> usize {
    if path == "/healthz" || path == "/readyz" {
        0
    } else if path == "/metrics" {
        1
    } else if path.starts_with("/v1/auth/") {
        2
    } else if path == "/v1/ingest/positions" {
        3
    } else if path == "/v1/ingest/heartbeat" {
        4
    } else if method == Method::GET
        && path.starts_with("/v1/")
        && (path.ends_with("/positions") || path.ends_with("/location"))
    {
        5
    } else if path.starts_with("/v1/ws/") || path == "/v1/realtime/tickets" {
        6
    } else if path.starts_with("/v1/") {
        7
    } else if path == "/" || path.starts_with("/assets/") {
        9
    } else {
        8
    }
}

fn add_security_headers(response: &mut Response) {
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
}

pub(crate) async fn security_rate_metrics(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let started = Instant::now();
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let route = route_class(&method, &path);
    let principal = request
        .headers()
        .get(header::AUTHORIZATION)
        .or_else(|| request.headers().get(header::COOKIE))
        .and_then(|value| value.to_str().ok())
        .unwrap_or("anonymous");
    let tenant = request
        .headers()
        .get("x-organization-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("no-tenant");
    let principal_key = crate::digest(principal)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let key_material = crate::digest(&format!("{principal}|{tenant}"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let peer_ip = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "local".to_owned());
    let remote = if state
        .auth
        .as_ref()
        .and_then(|auth| auth.trusted_proxy_ip)
        .is_some_and(|trusted| peer_ip == trusted.to_string())
    {
        request
            .headers()
            .get("x-real-ip")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<std::net::IpAddr>().ok())
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| peer_ip.clone())
    } else {
        peer_ip.clone()
    };
    let remote_key = crate::digest(&remote)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let limits: Vec<(String, usize)> = if route == 5 {
        vec![
            (format!("route:history:ip:{remote_key}"), 30),
            (format!("route:history:principal:{principal_key}"), 30),
            (format!("route:history:session:{}", key_material), 30),
        ]
    } else if route == 7 {
        vec![
            (format!("route:management:ip:{remote_key}"), 120),
            (format!("route:management:principal:{principal_key}"), 120),
            (format!("route:management:session:{}", key_material), 120),
        ]
    } else if route == 2 {
        let (key, limit) = if path == "/v1/auth/login" {
            ("route:login:ip", 30)
        } else {
            ("route:auth:ip", 60)
        };
        vec![(format!("{key}:{remote_key}"), limit)]
    } else if route == 6 {
        vec![(format!("route:realtime:ip:{remote_key}"), 120)]
    } else {
        Vec::new()
    };
    let secure_transport = request.uri().scheme_str() == Some("https")
        || (state
            .auth
            .as_ref()
            .and_then(|auth| auth.trusted_proxy_ip)
            .is_some_and(|trusted| peer_ip == trusted.to_string())
            && request
                .headers()
                .get("x-forwarded-proto")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.eq_ignore_ascii_case("https")));
    let limited = !limits.is_empty()
        && !take_limited_for(
            &state.auth_rate_limit,
            &limits,
            std::time::Duration::from_secs(if route == 2 && path == "/v1/auth/login" {
                15 * 60
            } else {
                60
            }),
        );
    let mut response = if limited {
        let mut response = StatusCode::TOO_MANY_REQUESTS.into_response();
        response.headers_mut().insert(
            header::RETRY_AFTER,
            HeaderValue::from_static(if route == 2 && path == "/v1/auth/login" {
                "900"
            } else {
                "60"
            }),
        );
        response
    } else {
        next.run(request).await
    };
    add_security_headers(&mut response);
    if secure_transport {
        response.headers_mut().insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    state.metrics.record(
        route,
        response.status(),
        started.elapsed().as_micros() as u64,
    );
    response
}

pub(crate) async fn metrics_endpoint(State(state): State<AppState>) -> Response {
    let mut response = state.metrics.render(&state).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    add_security_headers(&mut response);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use std::net::SocketAddr;
    use tower::ServiceExt;

    #[tokio::test]
    async fn security_headers_and_metrics_are_emitted_without_dynamic_labels() {
        let router = crate::app(AppState::new());
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["x-frame-options"], "DENY");
        assert!(
            response.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'")
        );
        assert!(!response.headers().contains_key("strict-transport-security"));

        let secure_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("https://tracker.example/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            secure_response.headers()["strict-transport-security"],
            "max-age=31536000"
        );

        let metrics = router
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(metrics.into_body(), 16_384)
            .await
            .unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("gps_http_requests_total{route=\"health\"} 2"));
        assert!(!body.contains("tenant_id"));
        assert!(!body.contains("device_id"));
    }

    #[tokio::test]
    async fn history_and_login_rates_return_retry_after() {
        let remote: SocketAddr = "192.0.2.12:1234".parse().unwrap();
        let router = crate::app(AppState::new());
        for _ in 0..30 {
            let mut request = Request::builder()
                .uri("/v1/devices/00000000-0000-4000-8000-000000000001/positions")
                .body(Body::empty())
                .unwrap();
            request
                .extensions_mut()
                .insert(axum::extract::ConnectInfo(remote));
            assert_ne!(
                router.clone().oneshot(request).await.unwrap().status(),
                StatusCode::TOO_MANY_REQUESTS
            );
        }
        let mut request = Request::builder()
            .uri("/v1/devices/00000000-0000-4000-8000-000000000001/positions")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(remote));
        let limited = router.clone().oneshot(request).await.unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(limited.headers()[header::RETRY_AFTER], "60");

        let login_router = crate::app(AppState::new());
        for _ in 0..30 {
            let mut request = Request::builder()
                .method(Method::POST)
                .uri("/v1/auth/login")
                .body(Body::empty())
                .unwrap();
            request
                .extensions_mut()
                .insert(axum::extract::ConnectInfo(remote));
            assert_ne!(
                login_router
                    .clone()
                    .oneshot(request)
                    .await
                    .unwrap()
                    .status(),
                StatusCode::TOO_MANY_REQUESTS
            );
        }
        let mut request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/login")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(axum::extract::ConnectInfo(remote));
        let limited = login_router.oneshot(request).await.unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(limited.headers()[header::RETRY_AFTER], "900");
    }
}
