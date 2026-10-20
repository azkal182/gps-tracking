use axum::{
    Json, Router,
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::{
    collections::{HashMap, VecDeque},
    env, fmt,
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

mod auth_routes;
pub mod email;
mod integrations;
mod mfa;
mod ops;
use auth_routes::*;
use integrations::*;
mod organizations;
use organizations::*;
pub use organizations::{
    grant_creator_object_access, require_application_token_owner_permission,
    require_object_permission,
};
pub use tracking::run_realtime_worker;
mod resources;
use resources::*;
mod tracking;
use tracking::*;

#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: Option<Secret>,
    pub public_origin: String,
    pub position_freshness_seconds: i64,
    pub device_offline_timeout_seconds: i64,
    pub position_retention_days: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    InvalidBind(String),
    InvalidMfaEncryptionKey,
    InvalidOutboxEncryptionKey,
    InvalidStatusThreshold(&'static str),
    InvalidPositionRetentionDays,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBind(_) => f.write_str("BIND_ADDR must be a valid socket address"),
            Self::InvalidMfaEncryptionKey => f.write_str("MFA_ENCRYPTION_KEY must be a URL-safe base64 encoded 256-bit key and MFA_ENCRYPTION_KEY_VERSION must be positive"),
            Self::InvalidOutboxEncryptionKey => f.write_str("OUTBOX_ENCRYPTION_KEY must be a URL-safe base64 encoded 256-bit key"),
            Self::InvalidStatusThreshold(name) => write!(f, "{name} must be a positive integer"),
            Self::InvalidPositionRetentionDays => f.write_str("POSITION_RETENTION_DAYS must be an integer between 1 and 36500"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
        let bind = SocketAddr::from_str(&bind)
            .map_err(|_| ConfigError::InvalidBind("value redacted".to_owned()))?;
        let database_url = env::var("DATABASE_URL").ok().map(Secret);
        if database_url.is_some() {
            let key = env::var("MFA_ENCRYPTION_KEY")
                .ok()
                .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
                .and_then(|value| <[u8; 32]>::try_from(value).ok());
            let version = match env::var("MFA_ENCRYPTION_KEY_VERSION") {
                Ok(value) => value.parse::<i16>().unwrap_or(0),
                Err(_) => 1,
            };
            if key.is_none() || version <= 0 {
                return Err(ConfigError::InvalidMfaEncryptionKey);
            }
            let outbox_key = env::var("OUTBOX_ENCRYPTION_KEY")
                .ok()
                .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
                .and_then(|value| <[u8; 32]>::try_from(value).ok());
            if outbox_key.is_none() {
                return Err(ConfigError::InvalidOutboxEncryptionKey);
            }
        }
        let public_origin =
            env::var("PUBLIC_ORIGIN").unwrap_or_else(|_| "http://localhost:5173".to_owned());
        let position_freshness_seconds = positive_seconds_env("POSITION_FRESHNESS_SECONDS", 60)?;
        let device_offline_timeout_seconds =
            positive_seconds_env("DEVICE_OFFLINE_TIMEOUT_SECONDS", 180)?;
        let position_retention_days = position_retention_days_env()?;
        Ok(Self {
            bind,
            database_url,
            public_origin,
            position_freshness_seconds,
            device_offline_timeout_seconds,
            position_retention_days,
        })
    }
}

/// Shared runtime pool safeguards: at most 20 connections, 5 seconds to acquire,
/// and a 10-second PostgreSQL statement timeout on every new connection.
pub fn database_pool_options() -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SET statement_timeout = '10s'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
}

#[derive(Clone)]
pub struct AppState {
    ready: Arc<AtomicBool>,
    auth: Option<AuthState>,
    tracking_pool: Option<PgPool>,
    hash_slots: Arc<tokio::sync::Semaphore>,
    auth_rate_limit: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
    heartbeat_rate_limit: Arc<Mutex<HashMap<String, HeartbeatRateBucket>>>,
    position_rate_limit: Arc<Mutex<HashMap<String, HeartbeatRateBucket>>>,
    metrics: Arc<ops::OperationalMetrics>,
    realtime_tickets: Arc<Mutex<HashMap<String, RealtimeTicket>>>,
    realtime_connections: Arc<Mutex<HashMap<String, usize>>>,
    realtime_clients: Arc<Mutex<HashMap<String, RealtimeConnection>>>,
    position_freshness_seconds: i64,
    device_offline_timeout_seconds: i64,
    position_retention_days: i32,
}

#[derive(Clone)]
pub(crate) struct RealtimeTicket {
    pub(crate) tenant_id: String,
    pub(crate) expires_at: Instant,
    pub(crate) principal: RealtimePrincipal,
}

#[derive(Clone)]
pub(crate) struct RealtimeEvent {
    pub(crate) tenant_id: String,
    pub(crate) device_id: Option<String>,
    pub(crate) target_id: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) message: serde_json::Value,
}

pub(crate) struct RealtimeConnection {
    pub(crate) tenant_id: String,
    pub(crate) subscriptions: SharedRealtimeSubscriptions,
    pub(crate) sender: tokio::sync::mpsc::Sender<RealtimeEvent>,
}

pub(crate) type RealtimeSubscriptions = (
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
);
pub(crate) type SharedRealtimeSubscriptions = Arc<Mutex<Option<RealtimeSubscriptions>>>;

#[derive(Clone)]
pub(crate) enum RealtimePrincipal {
    Session {
        session_id: String,
        user_id: String,
    },
    Application {
        token_id: String,
        owner_user_id: String,
        token_hash: Vec<u8>,
    },
}

#[derive(Clone, Copy)]
pub(crate) struct HeartbeatRateBucket {
    pub(crate) tokens: f64,
    pub(crate) updated_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    pub session_id: String,
    pub user_id: String,
    pub email: String,
    pub display_name: String,
}

/// Create the project-standard Argon2id PHC string for provisioning or password
/// changes. Call from a bounded blocking worker; never log the returned value.
pub fn hash_password(password: &[u8]) -> Result<String, argon2::password_hash::Error> {
    use argon2::PasswordHasher;
    use argon2::password_hash::SaltString;
    let params = argon2::Params::new(65_536, 3, 1, None)
        .map_err(|_| argon2::password_hash::Error::Crypto)?;
    let salt = SaltString::generate(&mut OsRng);
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password(password, &salt)
        .map(|hash| hash.to_string())
}

/// Validate the opaque browser session against revocation, auth version, idle
/// and absolute expiry. Interactive requests touch idle time at most once/minute.
pub async fn authenticate_cookie_session(
    pool: &PgPool,
    headers: &HeaderMap,
    interactive: bool,
) -> Result<AuthenticatedUser, StatusCode> {
    let secret =
        cookie_value(headers, "__Host-tracking_session").ok_or(StatusCode::UNAUTHORIZED)?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = sqlx::query_as::<_, (String, String, String, String, bool)>(
        "SELECT s.id::text,u.id::text,u.email_canonical,u.display_name, \
         s.last_interactive_at<=clock_timestamp()-interval '1 minute' \
         FROM auth_sessions s JOIN users u ON u.id=s.user_id \
         WHERE s.secret_hash=$1 AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() \
           AND s.last_interactive_at>clock_timestamp()-interval '30 minutes' \
           AND s.auth_version=u.auth_version AND u.status='active' FOR UPDATE OF s",
    )
    .bind(digest(secret))
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::UNAUTHORIZED)?;
    if interactive && row.4 {
        sqlx::query(
            "UPDATE auth_sessions SET last_interactive_at=clock_timestamp() WHERE id=$1::uuid",
        )
        .bind(&row.0)
        .execute(&mut *transaction)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    transaction
        .commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(AuthenticatedUser {
        session_id: row.0,
        user_id: row.1,
        email: row.2,
        display_name: row.3,
    })
}

#[derive(Clone)]
struct AuthState {
    pool: PgPool,
    platform_pool: Option<PgPool>,
    origin: String,
    mfa_key: Option<[u8; 32]>,
    outbox_key: Option<[u8; 32]>,
    mfa_key_version: i16,
    trusted_proxy_ip: Option<IpAddr>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            ready: Arc::new(AtomicBool::new(true)),
            auth: None,
            tracking_pool: None,
            hash_slots: Arc::new(tokio::sync::Semaphore::new(4)),
            auth_rate_limit: Arc::new(Mutex::new(HashMap::new())),
            heartbeat_rate_limit: Arc::new(Mutex::new(HashMap::new())),
            position_rate_limit: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(ops::OperationalMetrics::default()),
            realtime_tickets: Arc::new(Mutex::new(HashMap::new())),
            realtime_connections: Arc::new(Mutex::new(HashMap::new())),
            realtime_clients: Arc::new(Mutex::new(HashMap::new())),
            position_freshness_seconds: 60,
            device_offline_timeout_seconds: 180,
            position_retention_days: 365,
        }
    }
    pub fn with_auth(pool: PgPool, origin: String) -> Self {
        let mfa_key = env::var("MFA_ENCRYPTION_KEY")
            .ok()
            .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
            .and_then(|value| <[u8; 32]>::try_from(value).ok());
        let mfa_key_version = env::var("MFA_ENCRYPTION_KEY_VERSION")
            .ok()
            .and_then(|value| value.parse::<i16>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1);
        let outbox_key = env::var("OUTBOX_ENCRYPTION_KEY")
            .ok()
            .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
            .and_then(|value| <[u8; 32]>::try_from(value).ok());
        let trusted_proxy_ip = env::var("TRUSTED_PROXY_IP")
            .ok()
            .and_then(|value| value.parse().ok());
        Self {
            ready: Arc::new(AtomicBool::new(true)),
            auth: Some(AuthState {
                pool,
                platform_pool: None,
                origin,
                mfa_key,
                outbox_key,
                mfa_key_version,
                trusted_proxy_ip,
            }),
            tracking_pool: None,
            hash_slots: Arc::new(tokio::sync::Semaphore::new(4)),
            auth_rate_limit: Arc::new(Mutex::new(HashMap::new())),
            heartbeat_rate_limit: Arc::new(Mutex::new(HashMap::new())),
            position_rate_limit: Arc::new(Mutex::new(HashMap::new())),
            metrics: Arc::new(ops::OperationalMetrics::default()),
            realtime_tickets: Arc::new(Mutex::new(HashMap::new())),
            realtime_connections: Arc::new(Mutex::new(HashMap::new())),
            realtime_clients: Arc::new(Mutex::new(HashMap::new())),
            position_freshness_seconds: 60,
            device_offline_timeout_seconds: 180,
            position_retention_days: 365,
        }
    }
    #[cfg(test)]
    fn with_auth_test_key(pool: PgPool, origin: String, key: [u8; 32]) -> Self {
        let mut state = Self::with_auth(pool, origin);
        if let Some(auth) = state.auth.as_mut() {
            auth.mfa_key = Some(key);
        }
        state
    }
    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::SeqCst);
    }

    pub fn set_platform_pool(&mut self, pool: PgPool) {
        if let Some(auth) = self.auth.as_mut() {
            auth.platform_pool = Some(pool);
        }
    }

    pub fn set_tracking_pool(&mut self, pool: PgPool) {
        self.tracking_pool = Some(pool);
    }

    pub fn set_tracking_status_thresholds(
        &mut self,
        position_freshness_seconds: i64,
        device_offline_timeout_seconds: i64,
    ) {
        self.position_freshness_seconds = position_freshness_seconds;
        self.device_offline_timeout_seconds = device_offline_timeout_seconds;
    }

    pub fn set_position_retention_days(&mut self, days: i32) {
        self.position_retention_days = days;
    }
}

fn position_retention_days_env() -> Result<i32, ConfigError> {
    let value = env::var("POSITION_RETENTION_DAYS").ok();
    position_retention_days_value(value.as_deref())
}

fn position_retention_days_value(value: Option<&str>) -> Result<i32, ConfigError> {
    match value {
        None => Ok(365),
        Some(value) => value
            .parse::<i32>()
            .ok()
            .filter(|days| (1..=36_500).contains(days))
            .ok_or(ConfigError::InvalidPositionRetentionDays),
    }
}

fn positive_seconds_env(name: &'static str, default: i64) -> Result<i64, ConfigError> {
    positive_seconds_value(name, env::var(name).ok(), default)
}

fn positive_seconds_value(
    name: &'static str,
    value: Option<String>,
    default: i64,
) -> Result<i64, ConfigError> {
    match value {
        Some(value) => value
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(ConfigError::InvalidStatusThreshold(name)),
        None => Ok(default),
    }
}

pub async fn platform_pool_from_env() -> Result<Option<PgPool>, &'static str> {
    let Ok(url) = env::var("PLATFORM_DATABASE_URL") else {
        return Ok(None);
    };
    if url.trim().is_empty() {
        return Ok(None);
    }
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SET statement_timeout = '10s'")
                    .execute(&mut *connection)
                    .await?;
                sqlx::query("SET ROLE gps_tracking_platform_provisioner")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .map_err(|_| "platform database connection failed")?;
    Ok(Some(pool))
}

fn mfa_key_for_version(auth: &AuthState, version: i16) -> Option<[u8; 32]> {
    if version == auth.mfa_key_version {
        return auth.mfa_key;
    }
    env::var(format!("MFA_ENCRYPTION_KEY_{version}"))
        .ok()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .and_then(|value| <[u8; 32]>::try_from(value).ok())
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn healthz() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn readyz(state: axum::extract::State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    if state.ready.load(Ordering::SeqCst) {
        (StatusCode::OK, Json(HealthResponse { status: "ready" }))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not_ready",
            }),
        )
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(ops::metrics_endpoint))
        .route("/v1/auth/csrf", get(get_csrf))
        .route("/v1/auth/me", get(get_me))
        .route("/v1/auth/sessions", get(list_auth_sessions))
        .route("/v1/targets", get(list_targets).post(create_target))
        .route("/v1/targets/{id}", get(get_target).patch(update_target))
        .route("/v1/targets/{id}/location", get(get_target_location))
        .route("/v1/devices", get(list_devices).post(create_device))
        .route(
            "/v1/sessions",
            get(list_tracking_sessions).post(create_tracking_session),
        )
        .route("/v1/sessions/{session_id}", get(get_tracking_session))
        .route(
            "/v1/sessions/{session_id}/positions",
            get(get_session_positions),
        )
        .route("/v1/sessions/{session_id}/end", post(end_tracking_session))
        .route(
            "/v1/ingest/positions",
            post(ingest_positions).layer(axum::extract::DefaultBodyLimit::max(256 * 1024)),
        )
        .route(
            "/v1/ingest/heartbeat",
            post(ingest_heartbeat).layer(axum::extract::DefaultBodyLimit::max(1024)),
        )
        .route("/v1/realtime/tickets", post(create_realtime_ticket))
        .route("/v1/ws/tracking", get(tracking_websocket))
        .route("/v1/devices/{id}", get(get_device).patch(update_device))
        .route("/v1/devices/{id}/location", get(get_device_location))
        .route("/v1/devices/{id}/positions", get(get_device_positions))
        .route(
            "/v1/devices/{id}/credentials/rotate",
            post(rotate_device_credential),
        )
        .route("/v1/devices/{id}/credentials", get(list_device_credentials))
        .route(
            "/v1/devices/{id}/credentials/{credential_id}",
            axum::routing::delete(revoke_device_credential),
        )
        .route(
            "/v1/organization/application-tokens",
            get(list_application_tokens).post(issue_application_token),
        )
        .route(
            "/v1/organization/application-tokens/{id}",
            axum::routing::delete(revoke_application_token),
        )
        .route("/v1/organization/audit-events", get(list_audit_events))
        .route("/v1/organizations", get(list_my_organizations))
        .route(
            "/v1/platform/organizations",
            get(list_platform_organizations).post(create_organization),
        )
        .route(
            "/v1/platform/organizations/{id}",
            axum::routing::patch(set_organization_status),
        )
        .route(
            "/v1/platform/organizations/{id}/admin-invitations",
            post(reissue_initial_admin_invitation),
        )
        .route(
            "/v1/organization",
            get(get_organization).patch(update_organization),
        )
        .route(
            "/v1/organization/invitations",
            get(list_invitations).post(create_invitation),
        )
        .route("/v1/organization/members", get(list_members))
        .route(
            "/v1/organization/members/{user_id}",
            axum::routing::patch(update_member).delete(remove_member),
        )
        .route(
            "/v1/organization/members/{user_id}/grants",
            get(list_member_grants).put(replace_member_grants),
        )
        .route(
            "/v1/organization/invitations/{id}/resend",
            post(resend_invitation),
        )
        .route(
            "/v1/organization/invitations/{id}",
            axum::routing::delete(revoke_invitation),
        )
        .route("/v1/auth/invitations/accept", post(accept_invitation))
        .route("/v1/auth/activate", post(activate_account))
        .route(
            "/v1/auth/recovery/complete",
            post(complete_operator_recovery),
        )
        .route("/v1/auth/password/forgot", post(forgot_password))
        .route("/v1/auth/password/reset", post(reset_password))
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/mfa/verify", post(verify_mfa))
        .route("/v1/auth/mfa/enrollment", post(begin_mfa_enrollment))
        .route(
            "/v1/auth/mfa/enrollment/confirm",
            post(confirm_mfa_enrollment),
        )
        .route("/v1/auth/mfa/recover", post(recover_mfa))
        .route("/v1/auth/reauthenticate", post(reauthenticate))
        .route(
            "/v1/auth/mfa/recovery-codes/rotate",
            post(rotate_recovery_codes),
        )
        .route("/v1/auth/mfa", axum::routing::delete(disable_mfa))
        .route("/v1/auth/logout", post(logout))
        .route(
            "/v1/auth/sessions/{id}",
            axum::routing::delete(revoke_session),
        )
        .route_layer(middleware::from_fn(auth_no_store))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            ops::security_rate_metrics,
        ))
        .with_state(state)
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
