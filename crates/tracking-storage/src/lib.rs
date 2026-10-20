//! SQLx persistence primitives and embedded database migrations, including
//! tenant-scoped object-grant policy and membership lifecycle capabilities.

use sqlx::{PgPool, Postgres, Transaction, migrate::Migrator};
use std::time::Duration;

// Keep this source fingerprint coupled to the workspace migration directory;
// SQLx embeds the migration list at compile time, including newly added files.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RetentionReport {
    pub positions: u64,
    pub audit_events: u64,
    pub idempotency_records: u64,
    pub email_outbox: u64,
    pub auth_sessions: u64,
    pub auth_challenges: u64,
    pub account_tokens: u64,
    pub recovery_codes: u64,
}

/// Purge eligible rows in bounded transactions using a login principal granted
/// only `gps_tracking_retention_worker`. Returns an error on any failed batch so
/// the scheduler can alert; no location payload is selected into the process.
pub async fn purge_expired_data(
    pool: &PgPool,
    position_retention_days: i32,
) -> Result<RetentionReport, sqlx::Error> {
    let mut report = RetentionReport::default();
    let position_predicate = format!(
        "expires_at <= statement_timestamp() OR recorded_at < statement_timestamp() - ({position_retention_days} * interval '1 day')"
    );
    for (table, predicate, position_days, count) in [
        (
            "positions",
            position_predicate.as_str(),
            Some(position_retention_days),
            &mut report.positions,
        ),
        (
            "audit_events",
            "occurred_at < statement_timestamp() - interval '365 days'",
            None,
            &mut report.audit_events,
        ),
        (
            "idempotency_records",
            "expires_at <= statement_timestamp()",
            None,
            &mut report.idempotency_records,
        ),
        (
            "email_outbox",
            "retention_expires_at <= statement_timestamp()",
            None,
            &mut report.email_outbox,
        ),
        (
            "auth_sessions",
            "(expires_at <= statement_timestamp() OR revoked_at <= statement_timestamp()) AND coalesce(revoked_at, expires_at) < statement_timestamp() - interval '7 days'",
            None,
            &mut report.auth_sessions,
        ),
        (
            "auth_challenges",
            "(expires_at <= statement_timestamp() OR consumed_at <= statement_timestamp()) AND coalesce(consumed_at, expires_at) < statement_timestamp() - interval '7 days'",
            None,
            &mut report.auth_challenges,
        ),
        (
            "account_tokens",
            "(expires_at <= statement_timestamp() OR consumed_at <= statement_timestamp() OR revoked_at <= statement_timestamp()) AND least(expires_at, coalesce(consumed_at, expires_at), coalesce(revoked_at, expires_at)) < statement_timestamp() - interval '7 days'",
            None,
            &mut report.account_tokens,
        ),
        (
            "recovery_codes",
            "consumed_at < statement_timestamp() - interval '7 days'",
            None,
            &mut report.recovery_codes,
        ),
    ] {
        *count = purge_table(pool, table, predicate, position_days).await?;
    }
    Ok(report)
}

async fn purge_table(
    pool: &PgPool,
    table: &str,
    predicate: &str,
    position_retention_days: Option<i32>,
) -> Result<u64, sqlx::Error> {
    let query = format!(
        "WITH expired AS (SELECT id FROM public.{table} WHERE {predicate} ORDER BY id LIMIT 5000) DELETE FROM public.{table} AS row USING expired WHERE row.id = expired.id"
    );
    let mut total = 0;
    loop {
        let mut tx = pool.begin().await?;
        sqlx::query("SET LOCAL ROLE gps_tracking_retention_worker")
            .execute(&mut *tx)
            .await?;
        if let Some(days) = position_retention_days {
            sqlx::query("SELECT set_config('gps.position_retention_days', $1, true)")
                .bind(days.to_string())
                .execute(&mut *tx)
                .await?;
        }
        let result = sqlx::query(&query).execute(&mut *tx).await?;
        tx.commit().await?;
        let removed = result.rows_affected();
        total += removed;
        if removed < 5000 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Ok(total)
}

/// Apply all pending application migrations. Call during the one-shot migration job,
/// not automatically from every API process at startup.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

/// Begin a tenant-scoped transaction and install the RLS context transaction-locally.
/// The context is cleared by PostgreSQL when the transaction commits or rolls back.
pub async fn begin_tenant_transaction(
    pool: &PgPool,
    tenant_id: &str,
) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    if let Err(error) = set_tenant_context(&mut transaction, tenant_id).await {
        let _ = transaction.rollback().await;
        return Err(error);
    }
    Ok(transaction)
}

/// Set the RLS context for the transaction only; never use session-level SET.
pub async fn set_tenant_context(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT pg_catalog.set_config('app.tenant_id', $1::uuid::text, TRUE)",
    )
    .bind(tenant_id)
    .fetch_one(&mut **transaction)
    .await?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct ApplicationTokenIdentity {
    pub token_id: String,
    pub tenant_id: String,
    pub owner_user_id: String,
    pub scopes: Vec<String>,
}

/// Resolve an application bearer digest without exposing token hashes to the
/// tracking app role. Owner membership and organization status must be active.
pub async fn lookup_application_token(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<ApplicationTokenIdentity>, sqlx::Error> {
    sqlx::query_as::<_, (String, String, String, Vec<String>)>(
        "SELECT token_id::text,tenant_id::text,owner_user_id::text,scopes \
         FROM public.gps_tracking_lookup_application_token($1)",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .map(|identity| {
        identity.map(
            |(token_id, tenant_id, owner_user_id, scopes)| ApplicationTokenIdentity {
                token_id,
                tenant_id,
                owner_user_id,
                scopes,
            },
        )
    })
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeviceCredentialIdentity {
    pub tenant_id: String,
    pub device_id: String,
    pub ingestion_policy: String,
}

/// Resolve a device token digest through the narrowly scoped SECURITY DEFINER function.
/// Raw credential digests are not readable through the application role.
pub async fn lookup_device_credential(
    pool: &PgPool,
    token_hash: &[u8],
) -> Result<Option<DeviceCredentialIdentity>, sqlx::Error> {
    sqlx::query_as::<_, (String, String, String)>(
        "SELECT tenant_id::text, device_id::text, ingestion_policy \
         FROM public.lookup_device_credential($1)",
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .map(|identity| {
        identity.map(
            |(tenant_id, device_id, ingestion_policy)| DeviceCredentialIdentity {
                tenant_id,
                device_id,
                ingestion_policy,
            },
        )
    })
}

/// Acquire the agreed tracking lock order within one transaction: organization
/// shared, devices by UUID, targets by UUID, then sessions by UUID. Requests that
/// mutate organization state must lock the organization exclusively before any
/// child rows. All IDs are canonicalized before ordering so callers may pass them
/// in any order without creating opposite lock sequences.
pub async fn lock_tracking_resources(
    transaction: &mut Transaction<'_, Postgres>,
    tenant_id: &str,
    device_ids: &[String],
    target_ids: &[String],
    session_ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT id::text FROM organizations WHERE id = $1::uuid FOR SHARE",
    )
    .bind(tenant_id)
    .fetch_one(&mut **transaction)
    .await?;

    for device_id in ordered_uuid_text(device_ids) {
        sqlx::query_scalar::<_, String>(
            "SELECT id::text FROM devices \
             WHERE tenant_id = $1::uuid AND id = $2::uuid FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(device_id)
        .fetch_one(&mut **transaction)
        .await?;
    }

    for target_id in ordered_uuid_text(target_ids) {
        sqlx::query_scalar::<_, String>(
            "SELECT id::text FROM targets \
             WHERE tenant_id = $1::uuid AND id = $2::uuid FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(target_id)
        .fetch_one(&mut **transaction)
        .await?;
    }

    for session_id in ordered_uuid_text(session_ids) {
        sqlx::query_scalar::<_, String>(
            "SELECT id::text FROM tracking_sessions \
             WHERE tenant_id = $1::uuid AND id = $2::uuid FOR UPDATE",
        )
        .bind(tenant_id)
        .bind(session_id)
        .fetch_one(&mut **transaction)
        .await?;
    }

    Ok(())
}

fn ordered_uuid_text(ids: &[String]) -> Vec<String> {
    let mut ids = ids
        .iter()
        .map(|id| id.to_ascii_lowercase())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    ids
}

#[derive(Debug, PartialEq, Eq)]
pub struct AccountTokenClaim {
    pub user_id: String,
    pub purpose: String,
}

/// Consume a still-valid single-use account token inside the caller's transaction.
/// A `None` result means missing, expired, revoked, already consumed, or wrong purpose.
pub async fn consume_account_token(
    transaction: &mut Transaction<'_, Postgres>,
    digest: &[u8],
    purpose: &str,
) -> Result<Option<AccountTokenClaim>, sqlx::Error> {
    let claim = sqlx::query_as::<_, (String, String)>(
        "UPDATE account_tokens \
         SET consumed_at = clock_timestamp() \
         WHERE secret_hash = $1 AND purpose = $2 \
           AND consumed_at IS NULL AND revoked_at IS NULL \
           AND expires_at > clock_timestamp() \
         RETURNING user_id::text, purpose",
    )
    .bind(digest)
    .bind(purpose)
    .fetch_optional(&mut **transaction)
    .await?;

    Ok(claim.map(|(user_id, purpose)| AccountTokenClaim { user_id, purpose }))
}

/// Consume a recovery code once inside the caller's transaction.
pub async fn consume_recovery_code(
    transaction: &mut Transaction<'_, Postgres>,
    digest: &[u8],
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_as::<_, (String,)>(
        "UPDATE recovery_codes \
         SET consumed_at = clock_timestamp() \
         WHERE code_hash = $1 AND consumed_at IS NULL \
         RETURNING user_id::text",
    )
    .bind(digest)
    .fetch_optional(&mut **transaction)
    .await
    .map(|claim| claim.map(|(user_id,)| user_id))
}
