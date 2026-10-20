use tracking_storage::purge_expired_data;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("retentionctl=info")
        .with_target(false)
        .init();
    let url = std::env::var("RETENTION_DATABASE_URL")
        .map_err(|_| "RETENTION_DATABASE_URL is required")?;
    let retention_days = match std::env::var("POSITION_RETENTION_DAYS") {
        Ok(value) => value
            .parse::<i32>()
            .ok()
            .filter(|days| (1..=36_500).contains(days))
            .ok_or("POSITION_RETENTION_DAYS must be an integer between 1 and 36500")?,
        Err(_) => 365,
    };
    let pool = server::database_pool_options()
        .connect(&url)
        .await
        .map_err(|_| "retention database connection failed (details suppressed)")?;
    let report = purge_expired_data(&pool, retention_days)
        .await
        .map_err(|_| "retention purge failed; scheduler must alert (details suppressed)")?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_retention_worker")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('gps.position_retention_days', $1, true)")
        .bind(retention_days.to_string())
        .execute(&mut *tx)
        .await?;
    let lag_seconds: Option<f64> = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM (statement_timestamp() - min(recorded_at + ($1::integer * interval '1 day'))))::float8 FROM positions WHERE expires_at <= statement_timestamp() OR recorded_at < statement_timestamp() - ($1::integer * interval '1 day')",
    )
    .bind(retention_days)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    if lag_seconds.is_some_and(|seconds| seconds > 86_400.0) {
        tracing::error!("retention lag exceeds 24 hours; operator action required");
        return Err("retention lag alarm".into());
    }
    tracing::info!(
        positions = report.positions,
        audit_events = report.audit_events,
        idempotency_records = report.idempotency_records,
        email_outbox = report.email_outbox,
        auth_sessions = report.auth_sessions,
        auth_challenges = report.auth_challenges,
        account_tokens = report.account_tokens,
        recovery_codes = report.recovery_codes,
        "retention purge completed"
    );
    pool.close().await;
    Ok(())
}
