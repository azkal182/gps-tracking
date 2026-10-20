use server::{AppState, Config, app, run_realtime_worker};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!(address = %config.bind, "server listening");
    let mut state = if let Some(database_url) = config.database_url.as_ref() {
        let pool = server::database_pool_options()
            .connect(database_url.expose())
            .await
            .map_err(|_| "database connection failed (details suppressed)")?;
        AppState::with_auth(pool, config.public_origin.clone())
    } else {
        AppState::new()
    };
    state.set_tracking_status_thresholds(
        config.position_freshness_seconds,
        config.device_offline_timeout_seconds,
    );
    state.set_position_retention_days(config.position_retention_days);
    if let Ok(tracking_database_url) = std::env::var("TRACKING_DATABASE_URL") {
        let tracking_pool = server::database_pool_options()
            .connect(&tracking_database_url)
            .await
            .map_err(|_| "tracking database connection failed (details suppressed)")?;
        state.set_tracking_pool(tracking_pool);
    }
    if let Some(platform_pool) = server::platform_pool_from_env().await? {
        state.set_platform_pool(platform_pool);
    }
    if config.database_url.is_some() {
        tokio::spawn(run_realtime_worker(state.clone()));
    }
    let mailer = server::email::start_from_env(config.public_origin.clone()).await?;
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    if let Some(mailer) = mailer {
        mailer.abort();
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate()).expect("register SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("shutdown signal received");
}
