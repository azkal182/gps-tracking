use std::{env, fs, path::PathBuf, str::FromStr, time::Duration};

use sqlx::{PgPool, postgres::PgConnectOptions};
use tokio::sync::Barrier;
use tracking_storage::{
    MIGRATOR, begin_tenant_transaction, consume_account_token, consume_recovery_code,
    lock_tracking_resources, purge_expired_data, set_tenant_context,
};

const INITIAL_MIGRATION: &str = "202610190001_identity.sql";
const AUTH_MIGRATION: &str = "202610190002_membership_auth.sql";
const PASSWORD_HASH: &str = "$argon2id$v=19$m=65536,t=3,p=1$synthetic$synthetic-fixture-only";

#[tokio::test]
#[ignore = "requires a dedicated disposable PostgreSQL database via GPS_TEST_DATABASE_URL"]
async fn fresh_upgrade_uniqueness_missing_tenant_and_atomic_consumption()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let database_url = env::var("GPS_TEST_DATABASE_URL")?;
    let options = PgConnectOptions::from_str(&database_url)?;
    let pool = PgPool::connect_with(options).await?;

    // Apply the prior schema as an existing installation, then upgrade the domain migration.
    let migrations_table_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await?;
    if !migrations_table_exists {
        let migration_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
        let staging_dir = env::temp_dir().join(format!("gps-db01-stage1-{}", std::process::id()));
        fs::create_dir_all(&staging_dir)?;
        fs::copy(
            migration_root.join(INITIAL_MIGRATION),
            staging_dir.join(INITIAL_MIGRATION),
        )?;
        fs::copy(
            migration_root.join(AUTH_MIGRATION),
            staging_dir.join(AUTH_MIGRATION),
        )?;
        let stage_one = sqlx::migrate::Migrator::new(staging_dir.clone()).await?;
        stage_one.run(&pool).await?;
        let domain_exists: bool =
            sqlx::query_scalar("SELECT to_regclass('public.targets') IS NOT NULL")
                .fetch_one(&pool)
                .await?;
        assert!(
            !domain_exists,
            "the pre-domain schema has no tracking tables yet"
        );
        let _ = fs::remove_dir_all(staging_dir);
    }
    let identity_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public.organizations') IS NOT NULL")
            .fetch_one(&pool)
            .await?;
    assert!(
        identity_exists,
        "prior schema contains tenant identity tables"
    );
    let auth_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public.auth_sessions') IS NOT NULL")
            .fetch_one(&pool)
            .await?;
    assert!(auth_exists, "prior schema contains authentication tables");

    MIGRATOR.run(&pool).await?;
    MIGRATOR.run(&pool).await?;
    let domain_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public.targets') IS NOT NULL")
            .fetch_one(&pool)
            .await?;
    assert!(domain_exists, "upgrade adds tracking domain tables");
    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        applied,
        MIGRATOR.iter().count() as i64,
        "upgrade applies all identity, tracking, tenant, grant-policy and token-scope migrations exactly once"
    );
    let mailer_can_read_outbox: bool = sqlx::query_scalar(
        "SELECT has_column_privilege('gps_tracking_mailer','email_outbox','id','SELECT')",
    )
    .fetch_one(&pool)
    .await?;
    let mailer_can_read_positions: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('gps_tracking_mailer','positions','SELECT')",
    )
    .fetch_one(&pool)
    .await?;
    let mailer_can_read_tokens: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('gps_tracking_mailer','account_tokens','SELECT')",
    )
    .fetch_one(&pool)
    .await?;
    let auth_can_mint_bootstrap_guard: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('gps_tracking_auth','gps_tracking_platform_admin_exists()','EXECUTE')",
    )
    .fetch_one(&pool)
    .await?;
    let auth_can_insert_recovery_case: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('gps_tracking_auth','operator_recovery_cases','INSERT')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(mailer_can_read_outbox);
    assert!(
        !mailer_can_read_positions,
        "mailer capability must not reach location data"
    );
    assert!(
        !mailer_can_read_tokens,
        "mailer must not read authentication tokens"
    );
    assert!(auth_can_mint_bootstrap_guard && auth_can_insert_recovery_case);
    let grant_table_rls: bool = sqlx::query_scalar(
        "SELECT relrowsecurity AND relforcerowsecurity FROM pg_class WHERE oid='public.object_grants'::regclass",
    ).fetch_one(&pool).await?;
    let auth_can_read_grants: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('gps_tracking_auth','object_grants','SELECT')",
    )
    .fetch_one(&pool)
    .await?;
    let auth_can_execute_membership_mutation: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('gps_tracking_auth','gps_tracking_update_membership(uuid,uuid,uuid,uuid,text,text,boolean)','EXECUTE')",
    ).fetch_one(&pool).await?;
    let policy_owner_can_read_positions: bool = sqlx::query_scalar(
        "SELECT has_table_privilege('gps_tracking_grant_policy_owner','positions','SELECT')",
    )
    .fetch_one(&pool)
    .await?;
    let auth_policy_requires_mfa_fn: bool = sqlx::query_scalar(
        "SELECT has_function_privilege('gps_tracking_auth','gps_tracking_user_requires_mfa(uuid)','EXECUTE')",
    ).fetch_one(&pool).await?;
    assert!(grant_table_rls, "object grants use FORCE RLS");
    assert!(
        !auth_can_read_grants,
        "auth reads grants only through narrow functions"
    );
    assert!(
        auth_can_execute_membership_mutation,
        "membership writes use guarded function"
    );
    assert!(
        !policy_owner_can_read_positions,
        "grant policy owner cannot read location data"
    );
    assert!(
        auth_policy_requires_mfa_fn,
        "auth policy stays a boolean-only capability"
    );

    let org_id: String = sqlx::query_scalar(
        "INSERT INTO organizations (id, name) VALUES (gen_random_uuid(), 'Synthetic tenant') RETURNING id::text",
    )
    .fetch_one(&pool)
    .await?;
    let user_id: String = sqlx::query_scalar(
        "WITH user_identity AS (SELECT gen_random_uuid() AS id) \
         INSERT INTO users (id, email_canonical, display_name, password_hash) \
         SELECT id, id::text || '@example.invalid', 'Synthetic User', $1 FROM user_identity RETURNING id::text",
    )
    .bind(PASSWORD_HASH)
    .fetch_one(&pool)
    .await?;

    let missing_tenant = sqlx::query(
        "INSERT INTO memberships (id, tenant_id, user_id, role) VALUES (gen_random_uuid(), NULL, $1::uuid, 'viewer')",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .expect_err("membership tenant must not be omitted");
    assert_eq!(
        missing_tenant
            .as_database_error()
            .and_then(|err| err.code())
            .as_deref(),
        Some("23502")
    );

    let wrong_tenant = sqlx::query(
        "INSERT INTO memberships (id, tenant_id, user_id, role) VALUES (gen_random_uuid(), gen_random_uuid(), $1::uuid, 'viewer')",
    )
    .bind(&user_id)
    .execute(&pool)
    .await
    .expect_err("membership must reference an existing tenant");
    assert_eq!(
        wrong_tenant
            .as_database_error()
            .and_then(|err| err.code())
            .as_deref(),
        Some("23503")
    );

    let membership_id: String = sqlx::query_scalar(
        "INSERT INTO memberships (id, tenant_id, user_id, role, status) \
         VALUES (gen_random_uuid(), $1::uuid, $2::uuid, 'viewer', 'active') RETURNING id::text",
    )
    .bind(&org_id)
    .bind(&user_id)
    .fetch_one(&pool)
    .await?;
    let duplicate_membership = sqlx::query(
        "INSERT INTO memberships (id, tenant_id, user_id, role) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, 'viewer')",
    )
    .bind(&org_id)
    .bind(&user_id)
    .execute(&pool)
    .await
    .expect_err("tenant/user membership must be unique");
    assert_eq!(
        duplicate_membership
            .as_database_error()
            .and_then(|err| err.code())
            .as_deref(),
        Some("23505")
    );
    assert!(!membership_id.is_empty());

    let account_token_digest: Vec<u8> = sqlx::query_scalar(
        "INSERT INTO account_tokens (id, user_id, purpose, secret_hash, expires_at) \
         VALUES (gen_random_uuid(), $1::uuid, 'password_reset', \
             decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), \
             clock_timestamp() + interval '30 minutes') RETURNING secret_hash",
    )
    .bind(&user_id)
    .fetch_one(&pool)
    .await?;
    let recovery_code_digest: Vec<u8> = sqlx::query_scalar(
        "INSERT INTO recovery_codes (id, user_id, code_hash) \
         VALUES (gen_random_uuid(), $1::uuid, \
             decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex')) RETURNING code_hash",
    )
    .bind(&user_id)
    .fetch_one(&pool)
    .await?;

    let (token_claims, recovery_claims) = tokio::join!(
        consume_twice(&pool, account_token_digest, true),
        consume_twice(&pool, recovery_code_digest, false),
    );
    assert_eq!(
        token_claims?, 1,
        "only one racing account-token consumer succeeds"
    );
    assert_eq!(
        recovery_claims?, 1,
        "only one racing recovery-code consumer succeeds"
    );

    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a dedicated disposable PostgreSQL database via GPS_TEST_DATABASE_URL"]
async fn mfa_pending_membership_activation_uses_narrow_auth_capability()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let database_url = env::var("GPS_TEST_DATABASE_URL")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(PgConnectOptions::from_str(&database_url)?)
        .await?;
    MIGRATOR.run(&pool).await?;
    let org:String=sqlx::query_scalar("INSERT INTO organizations(id,name) VALUES(gen_random_uuid(),'Synthetic MFA activation') RETURNING id::text").fetch_one(&pool).await?;
    let user:String=sqlx::query_scalar("INSERT INTO users(id,email_canonical,display_name,password_hash) VALUES(gen_random_uuid(),gen_random_uuid()::text || '@example.invalid','Synthetic MFA',$1) RETURNING id::text").bind(PASSWORD_HASH).fetch_one(&pool).await?;
    let no_factor_user:String=sqlx::query_scalar("INSERT INTO users(id,email_canonical,display_name,password_hash) VALUES(gen_random_uuid(),gen_random_uuid()::text || '@example.invalid','Synthetic no factor',$1) RETURNING id::text").bind(PASSWORD_HASH).fetch_one(&pool).await?;
    for user_id in [&user, &no_factor_user] {
        sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES(gen_random_uuid(),$1::uuid,$2::uuid,'organization_admin','pending_mfa')").bind(&org).bind(user_id).execute(&pool).await?;
    }
    sqlx::query("INSERT INTO mfa_factors(id,user_id,encrypted_secret,nonce,key_version,enabled_at) VALUES(gen_random_uuid(),$1::uuid,decode(repeat('ab',16),'hex'),decode(repeat('cd',12),'hex'),1,clock_timestamp())").bind(&user).execute(&pool).await?;
    let mut connection = pool.acquire().await?;
    sqlx::query("SET ROLE gps_tracking_auth")
        .execute(&mut *connection)
        .await?;
    let direct_update =
        sqlx::query("UPDATE memberships SET status='active' WHERE user_id=$1::uuid")
            .bind(&user)
            .execute(&mut *connection)
            .await
            .expect_err("auth capability cannot directly update membership state");
    assert_sqlstate(&direct_update, "42501");
    let activated: i32 =
        sqlx::query_scalar("SELECT public.gps_tracking_activate_pending_mfa_memberships($1::uuid)")
            .bind(&user)
            .fetch_one(&mut *connection)
            .await?;
    assert_eq!(activated, 1);
    let without_factor: i32 =
        sqlx::query_scalar("SELECT public.gps_tracking_activate_pending_mfa_memberships($1::uuid)")
            .bind(&no_factor_user)
            .fetch_one(&mut *connection)
            .await?;
    assert_eq!(
        without_factor, 0,
        "pending account without an enabled factor stays restricted"
    );
    sqlx::query("RESET ROLE").execute(&mut *connection).await?;
    drop(connection);
    let status: String = sqlx::query_scalar(
        "SELECT status FROM memberships WHERE user_id=$1::uuid AND tenant_id=$2::uuid",
    )
    .bind(&user)
    .bind(&org)
    .fetch_one(&pool)
    .await?;
    assert_eq!(status, "active");
    let restricted_status: String = sqlx::query_scalar(
        "SELECT status FROM memberships WHERE user_id=$1::uuid AND tenant_id=$2::uuid",
    )
    .bind(&no_factor_user)
    .bind(&org)
    .fetch_one(&pool)
    .await?;
    assert_eq!(restricted_status, "pending_mfa");
    sqlx::query("DELETE FROM memberships WHERE tenant_id=$1::uuid")
        .bind(&org)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM organizations WHERE id=$1::uuid")
        .bind(&org)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM users WHERE id=ANY(ARRAY[$1::uuid,$2::uuid])")
        .bind(&user)
        .bind(&no_factor_user)
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a dedicated disposable PostgreSQL/PostGIS database via GPS_TEST_DATABASE_URL"]
async fn tenant_rls_roles_lookup_worker_and_pool_reuse()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let database_url = env::var("GPS_TEST_DATABASE_URL")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(PgConnectOptions::from_str(&database_url)?)
        .await?;
    MIGRATOR.run(&pool).await?;

    for role in [
        "gps_tracking_app",
        "gps_tracking_schema_owner",
        "gps_tracking_device_lookup_owner",
        "gps_tracking_retention_worker",
        "gps_tracking_platform_provisioner",
        "gps_tracking_auth",
        "gps_tracking_mfa_activation_owner",
        "gps_tracking_org_policy_owner",
        "gps_tracking_invitation_owner",
    ] {
        let bypass_rls: bool =
            sqlx::query_scalar("SELECT rolbypassrls FROM pg_roles WHERE rolname = $1")
                .bind(role)
                .fetch_one(&pool)
                .await?;
        assert_eq!(
            bypass_rls,
            matches!(
                role,
                "gps_tracking_device_lookup_owner"
                    | "gps_tracking_platform_provisioner"
                    | "gps_tracking_mfa_activation_owner"
                    | "gps_tracking_org_policy_owner"
                    | "gps_tracking_invitation_owner"
            ),
            "only the narrow lookup and provisioning capabilities can bypass RLS"
        );
    }
    let forced_rls: bool = sqlx::query_scalar(
        "SELECT relforcerowsecurity FROM pg_class WHERE oid = 'public.positions'::regclass",
    )
    .fetch_one(&pool)
    .await?;
    assert!(
        forced_rls,
        "positions enforce RLS even for their table owner"
    );
    let positions_owner: String = sqlx::query_scalar(
        "SELECT pg_catalog.pg_get_userbyid(relowner) FROM pg_class WHERE oid = 'public.positions'::regclass",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(positions_owner, "gps_tracking_schema_owner");
    let app_can_assume_lookup_owner: bool = sqlx::query_scalar(
        "SELECT pg_catalog.pg_has_role('gps_tracking_app', 'gps_tracking_device_lookup_owner', 'MEMBER')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(!app_can_assume_lookup_owner);
    let app_can_assume_provisioner: bool = sqlx::query_scalar(
        "SELECT pg_catalog.pg_has_role('gps_tracking_app', 'gps_tracking_platform_provisioner', 'MEMBER')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(!app_can_assume_provisioner);
    let app_can_assume_auth: bool = sqlx::query_scalar(
        "SELECT pg_catalog.pg_has_role('gps_tracking_app', 'gps_tracking_auth', 'MEMBER')",
    )
    .fetch_one(&pool)
    .await?;
    assert!(!app_can_assume_auth);
    let platform_can_read_mail_payload: bool = sqlx::query_scalar(
        "SELECT has_column_privilege('gps_tracking_platform_provisioner','email_outbox','encrypted_payload','SELECT')",
    ).fetch_one(&pool).await?;
    let platform_can_read_mail_routing_metadata: bool = sqlx::query_scalar(
        "SELECT has_column_privilege('gps_tracking_platform_provisioner','email_outbox','recipient','SELECT') AND has_column_privilege('gps_tracking_platform_provisioner','email_outbox','template','SELECT')",
    ).fetch_one(&pool).await?;
    assert!(
        !platform_can_read_mail_payload,
        "platform provisioning cannot decrypt or read queued email payloads"
    );
    assert!(
        platform_can_read_mail_routing_metadata,
        "platform provisioning may delete stale invitation mail by narrow routing metadata"
    );
    let rls_table_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class \
         WHERE relnamespace = 'public'::regnamespace \
           AND relname = ANY(ARRAY['organizations','memberships','invitations','email_outbox', \
             'targets','devices','device_credentials','application_tokens','application_token_devices', \
             'application_token_targets','tracking_sessions','positions','idempotency_records','audit_events']) \
           AND relrowsecurity AND relforcerowsecurity",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        rls_table_count, 14,
        "every current tenant table uses FORCE RLS"
    );

    let tenant_a = create_tenant(&pool, "RLS tenant A").await?;
    let tenant_b = create_tenant(&pool, "RLS tenant B").await?;
    let device_a = create_device(&pool, &tenant_a, "RLS device", "http", "rls-device").await?;
    let target_a = create_target(&pool, &tenant_a, "Tenant A target", None).await?;
    let target_b = create_target(&pool, &tenant_b, "Tenant B target", None).await?;
    let auth_user: String = sqlx::query_scalar(
        "INSERT INTO users (id, email_canonical, display_name, password_hash) \
         VALUES (gen_random_uuid(), gen_random_uuid()::text || '@example.invalid', 'RLS Auth User', $1) RETURNING id::text",
    )
    .bind(PASSWORD_HASH)
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO memberships (id, tenant_id, user_id, role, status) \
         VALUES (gen_random_uuid(), $1::uuid, $2::uuid, 'viewer', 'active')",
    )
    .bind(&tenant_a)
    .bind(&auth_user)
    .execute(&pool)
    .await?;
    let credential_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex')",
    )
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO device_credentials (id, tenant_id, device_id, token_hash) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, $3)",
    )
    .bind(&tenant_a)
    .bind(&device_a)
    .bind(&credential_hash)
    .execute(&pool)
    .await?;

    let expired_position =
        seed_position(&pool, &tenant_a, &device_a, "2024-01-01T00:00:00Z").await?;
    let policy_expired_position =
        seed_position(&pool, &tenant_a, &device_a, "2026-08-01T00:00:00Z").await?;
    let live_position = seed_position(&pool, &tenant_a, &device_a, "2026-10-19T00:00:00Z").await?;
    let device_b = create_device(&pool, &tenant_b, "Device B", "http", "rls-b").await?;
    seed_position(&pool, &tenant_b, &device_b, "2026-10-19T00:00:00Z").await?;

    let mut tenant_tx = begin_tenant_transaction(&pool, &tenant_a).await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_app")
        .execute(&mut *tenant_tx)
        .await?;
    let visible_a: i64 = sqlx::query_scalar("SELECT count(*) FROM targets")
        .fetch_one(&mut *tenant_tx)
        .await?;
    let visible_b: i64 = sqlx::query_scalar("SELECT count(*) FROM targets WHERE id = $1::uuid")
        .bind(&target_b)
        .fetch_one(&mut *tenant_tx)
        .await?;
    assert_eq!((visible_a, visible_b), (1, 0));
    tenant_tx.commit().await?;

    let mut auth_context = begin_tenant_transaction(&pool, &tenant_a).await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_auth")
        .execute(&mut *auth_context)
        .await?;
    let auth_memberships: i64 = sqlx::query_scalar("SELECT count(*) FROM memberships")
        .fetch_one(&mut *auth_context)
        .await?;
    let auth_users: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE id = $1::uuid")
        .bind(&auth_user)
        .fetch_one(&mut *auth_context)
        .await?;
    assert_eq!((auth_memberships, auth_users), (1, 1));
    let auth_location_denied = sqlx::query("SELECT id FROM positions LIMIT 1")
        .fetch_all(&mut *auth_context)
        .await
        .expect_err("auth capability cannot read tracking locations");
    assert_sqlstate(&auth_location_denied, "42501");
    auth_context.rollback().await?;

    let mut auth_missing_context = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_auth")
        .execute(&mut *auth_missing_context)
        .await?;
    let auth_without_tenant: i64 = sqlx::query_scalar("SELECT count(*) FROM memberships")
        .fetch_one(&mut *auth_missing_context)
        .await?;
    assert_eq!(auth_without_tenant, 0);
    auth_missing_context.rollback().await?;

    let mut missing_context = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_app")
        .execute(&mut *missing_context)
        .await?;
    let no_context_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM targets")
        .fetch_one(&mut *missing_context)
        .await?;
    assert_eq!(
        no_context_rows, 0,
        "missing tenant context must fail closed"
    );
    missing_context.rollback().await?;

    let mut missing_context_write = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_app")
        .execute(&mut *missing_context_write)
        .await?;
    let missing_context_error = sqlx::query(
        "INSERT INTO targets (id, tenant_id, name) VALUES (gen_random_uuid(), $1::uuid, 'missing-context')",
    )
    .bind(&tenant_a)
    .execute(&mut *missing_context_write)
    .await
    .expect_err("missing tenant context must reject writes");
    assert_sqlstate(&missing_context_error, "42501");
    missing_context_write.rollback().await?;

    let mut denied_write = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_app")
        .execute(&mut *denied_write)
        .await?;
    set_tenant_context(&mut denied_write, &tenant_a).await?;
    let cross_tenant_write = sqlx::query(
        "INSERT INTO targets (id, tenant_id, name) VALUES (gen_random_uuid(), $1::uuid, 'forbidden')",
    )
    .bind(&tenant_b)
    .execute(&mut *denied_write)
    .await
    .expect_err("tenant app cannot write a row for another tenant");
    assert_sqlstate(&cross_tenant_write, "42501");
    denied_write.rollback().await?;

    let mut owner_without_context = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_schema_owner")
        .execute(&mut *owner_without_context)
        .await?;
    let owner_without_context_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM targets")
        .fetch_one(&mut *owner_without_context)
        .await?;
    assert_eq!(
        owner_without_context_rows, 0,
        "FORCE RLS constrains the table owner"
    );
    owner_without_context.commit().await?;

    let mut owner_with_context = begin_tenant_transaction(&pool, &tenant_b).await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_schema_owner")
        .execute(&mut *owner_with_context)
        .await?;
    let owner_visible: i64 = sqlx::query_scalar("SELECT count(*) FROM targets")
        .fetch_one(&mut *owner_with_context)
        .await?;
    assert_eq!(
        owner_visible, 1,
        "owner policy still restricts rows to its context"
    );
    owner_with_context.commit().await?;

    let mut cross_tenant_fk = begin_tenant_transaction(&pool, &tenant_b).await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_app")
        .execute(&mut *cross_tenant_fk)
        .await?;
    let foreign_key_error = sqlx::query(
        "INSERT INTO positions (tenant_id, device_id, payload_fingerprint, location, recorded_at) VALUES ($1::uuid, $2::uuid, decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), ST_SetSRID(ST_MakePoint(1, 1), 4326)::geography, clock_timestamp())",
    )
    .bind(&tenant_b)
    .bind(&device_a)
    .execute(&mut *cross_tenant_fk)
    .await
    .expect_err("tenant-composite FK rejects a device owned by another tenant");
    assert_sqlstate(&foreign_key_error, "23503");
    cross_tenant_fk.rollback().await?;

    let identity = tracking_storage::lookup_device_credential(&pool, &credential_hash)
        .await?
        .expect("active credential resolves to a device identity");
    assert_eq!(identity.tenant_id, tenant_a);
    assert_eq!(identity.device_id, device_a);
    assert_eq!(identity.ingestion_policy, "session_only");
    sqlx::query("UPDATE devices SET enabled = FALSE WHERE tenant_id = $1::uuid AND id = $2::uuid")
        .bind(&tenant_a)
        .bind(&device_a)
        .execute(&pool)
        .await?;
    assert!(
        tracking_storage::lookup_device_credential(&pool, &credential_hash)
            .await?
            .is_none(),
        "disabled devices must not resolve a credential"
    );
    sqlx::query("UPDATE devices SET enabled = TRUE WHERE tenant_id = $1::uuid AND id = $2::uuid")
        .bind(&tenant_a)
        .bind(&device_a)
        .execute(&pool)
        .await?;

    let mut secret_denied = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_app")
        .execute(&mut *secret_denied)
        .await?;
    let resolved: (String, String, String) = sqlx::query_as(
        "SELECT tenant_id::text, device_id::text, ingestion_policy \
         FROM public.lookup_device_credential($1)",
    )
    .bind(&credential_hash)
    .fetch_one(&mut *secret_denied)
    .await?;
    assert_eq!(
        resolved,
        (
            tenant_a.clone(),
            device_a.clone(),
            "session_only".to_owned()
        )
    );
    let raw_digest_read = sqlx::query("SELECT token_hash FROM device_credentials LIMIT 1")
        .fetch_all(&mut *secret_denied)
        .await
        .expect_err("application role cannot read stored credential digests");
    assert_sqlstate(&raw_digest_read, "42501");
    secret_denied.rollback().await?;

    let visible_before_reuse: i64 = {
        let mut tx = begin_tenant_transaction(&pool, &tenant_a).await?;
        sqlx::query("SET LOCAL ROLE gps_tracking_app")
            .execute(&mut *tx)
            .await?;
        let count = sqlx::query_scalar("SELECT count(*) FROM targets")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        count
    };
    assert_eq!(visible_before_reuse, 1);
    let mut reused_connection = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_app")
        .execute(&mut *reused_connection)
        .await?;
    let visible_after_reuse: i64 = sqlx::query_scalar("SELECT count(*) FROM targets")
        .fetch_one(&mut *reused_connection)
        .await?;
    assert_eq!(
        visible_after_reuse, 0,
        "transaction-local tenant context does not leak through the pool"
    );
    reused_connection.rollback().await?;

    let mut retention_worker = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_retention_worker")
        .execute(&mut *retention_worker)
        .await?;
    sqlx::query("SELECT set_config('gps.position_retention_days', '60', true)")
        .execute(&mut *retention_worker)
        .await?;
    let policy_expired_deleted: Option<i64> =
        sqlx::query_scalar("DELETE FROM positions WHERE id = $1 RETURNING id")
            .bind(policy_expired_position)
            .fetch_optional(&mut *retention_worker)
            .await?;
    let expired_deleted: Option<i64> =
        sqlx::query_scalar("DELETE FROM positions WHERE id = $1 RETURNING id")
            .bind(expired_position)
            .fetch_optional(&mut *retention_worker)
            .await?;
    let live_deleted: Option<i64> =
        sqlx::query_scalar("DELETE FROM positions WHERE id = $1 RETURNING id")
            .bind(live_position)
            .fetch_optional(&mut *retention_worker)
            .await?;
    assert_eq!(expired_deleted, Some(expired_position));
    assert_eq!(
        policy_expired_deleted,
        Some(policy_expired_position),
        "worker applies the configured 60-day cutoff to rows originally stored with 365-day expiry"
    );
    assert_eq!(
        live_deleted, None,
        "worker cannot delete unexpired positions"
    );
    let location_read_denied = sqlx::query("SELECT location FROM positions LIMIT 1")
        .fetch_all(&mut *retention_worker)
        .await
        .expect_err("retention worker cannot read location payloads");
    assert_sqlstate(&location_read_denied, "42501");
    retention_worker.commit().await?;

    let mut worker_scope = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_retention_worker")
        .execute(&mut *worker_scope)
        .await?;
    let forbidden_target_delete = sqlx::query("DELETE FROM targets WHERE id = $1::uuid")
        .bind(&target_a)
        .execute(&mut *worker_scope)
        .await
        .expect_err("retention worker has no target mutation privilege");
    assert_sqlstate(&forbidden_target_delete, "42501");
    worker_scope.rollback().await?;

    let mut provisioner = pool.begin().await?;
    sqlx::query("SET LOCAL ROLE gps_tracking_platform_provisioner")
        .execute(&mut *provisioner)
        .await?;
    let provisioned_org: String = sqlx::query_scalar(
        "INSERT INTO organizations (id, name) VALUES (gen_random_uuid(), 'Provisioned synthetic org') RETURNING id::text",
    )
    .fetch_one(&mut *provisioner)
    .await?;
    assert!(!provisioned_org.is_empty());
    let location_denied = sqlx::query("SELECT id FROM positions LIMIT 1")
        .fetch_all(&mut *provisioner)
        .await
        .expect_err("platform provisioner has no location read privilege");
    assert_sqlstate(&location_denied, "42501");
    provisioner.rollback().await?;

    pool.close().await;
    Ok(())
}

async fn consume_twice(
    pool: &PgPool,
    digest: Vec<u8>,
    is_account_token: bool,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let barrier = std::sync::Arc::new(Barrier::new(2));
    let consume_one = |pool: PgPool, digest: Vec<u8>, barrier: std::sync::Arc<Barrier>| async move {
        let mut tx = pool.begin().await?;
        barrier.wait().await;
        let consumed = if is_account_token {
            consume_account_token(&mut tx, &digest, "password_reset")
                .await?
                .is_some()
        } else {
            consume_recovery_code(&mut tx, &digest).await?.is_some()
        };
        tx.commit().await?;
        Ok::<bool, sqlx::Error>(consumed)
    };
    let first = consume_one(pool.clone(), digest.clone(), barrier.clone());
    let second = consume_one(pool.clone(), digest, barrier);
    let (first, second) = tokio::join!(first, second);
    Ok(usize::from(first?) + usize::from(second?))
}

#[tokio::test]
#[ignore = "requires a dedicated disposable PostgreSQL/PostGIS database via GPS_TEST_DATABASE_URL"]
async fn tracking_domain_constraints_retention_and_concurrency()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let database_url = env::var("GPS_TEST_DATABASE_URL")?;
    let pool = PgPool::connect_with(PgConnectOptions::from_str(&database_url)?).await?;
    MIGRATOR.run(&pool).await?;

    let tenant = create_tenant(&pool, "DB-02 tenant").await?;
    let other_tenant = create_tenant(&pool, "DB-02 other tenant").await?;
    let target = create_target(&pool, &tenant, "Target A", Some("external-A")).await?;
    let other_target = create_target(&pool, &tenant, "Target B", None).await?;
    let device = create_device(&pool, &tenant, "Device A", "http", "tracker-A").await?;
    let other_device = create_device(&pool, &tenant, "Device B", "http", "tracker-B").await?;
    let foreign_device =
        create_device(&pool, &other_tenant, "Foreign device", "http", "foreign").await?;

    let duplicate_external_ref = sqlx::query(
        "INSERT INTO targets (id, tenant_id, name, external_ref) VALUES (gen_random_uuid(), $1::uuid, 'Duplicate ref', 'external-A')",
    )
    .bind(&tenant)
    .execute(&pool)
    .await
    .expect_err("target external_ref is unique within a tenant");
    assert_sqlstate(&duplicate_external_ref, "23505");

    let invalid_session_tenant = sqlx::query(
        "INSERT INTO tracking_sessions (id, tenant_id, target_id, device_id) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, $3::uuid)",
    )
    .bind(&other_tenant)
    .bind(&target)
    .bind(&foreign_device)
    .execute(&pool)
    .await
    .expect_err("session target and device must belong to its tenant");
    assert_sqlstate(&invalid_session_tenant, "23503");

    let digest = "decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex')";
    sqlx::query(&format!(
        "INSERT INTO device_credentials (id, tenant_id, device_id, token_hash) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, {digest})"
    ))
    .bind(&tenant)
    .bind(&device)
    .execute(&pool)
    .await?;
    let second_active_credential = sqlx::query(&format!(
        "INSERT INTO device_credentials (id, tenant_id, device_id, token_hash) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, {digest})"
    ))
    .bind(&tenant)
    .bind(&device)
    .execute(&pool)
    .await
    .expect_err("a device can have only one active credential");
    assert_sqlstate(&second_active_credential, "23505");

    let user_id: String = sqlx::query_scalar(
        "INSERT INTO users (id, email_canonical, display_name, password_hash) VALUES (gen_random_uuid(), gen_random_uuid()::text || '@example.invalid', 'Token Owner', $1) RETURNING id::text",
    )
    .bind(PASSWORD_HASH)
    .fetch_one(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO memberships (id, tenant_id, user_id, role, status) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, 'organization_admin', 'active')",
    )
    .bind(&tenant)
    .bind(&user_id)
    .execute(&pool)
    .await?;
    let mut app_token_tx = pool.begin().await?;
    let app_token: String = sqlx::query_scalar(
        "INSERT INTO application_tokens (id, tenant_id, owner_user_id, name, token_hash, scopes, expires_at) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, 'device scope', decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), ARRAY['positions:read'], clock_timestamp() + interval '30 days') RETURNING id::text",
    )
    .bind(&tenant)
    .bind(&user_id)
    .fetch_one(&mut *app_token_tx)
    .await?;
    sqlx::query(
        "INSERT INTO application_token_devices (tenant_id, application_token_id, device_id) VALUES ($1::uuid, $2::uuid, $3::uuid)",
    )
    .bind(&tenant)
    .bind(&app_token)
    .bind(&device)
    .execute(&mut *app_token_tx)
    .await?;
    app_token_tx.commit().await?;

    let empty_allowlist = sqlx::query(
        "INSERT INTO application_tokens (id, tenant_id, owner_user_id, name, token_hash, scopes, expires_at) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, 'missing allowlist', decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), ARRAY['positions:read'], clock_timestamp() + interval '30 days')",
    )
    .bind(&tenant)
    .bind(&user_id)
    .execute(&pool)
    .await
    .expect_err("application tokens must have an explicit resource allowlist");
    assert_sqlstate(&empty_allowlist, "23514");

    let concurrent_barrier = std::sync::Arc::new(Barrier::new(2));
    let first_start = attempt_session_start(
        pool.clone(),
        tenant.clone(),
        device.clone(),
        target.clone(),
        concurrent_barrier.clone(),
    );
    let second_start = attempt_session_start(
        pool.clone(),
        tenant.clone(),
        device.clone(),
        target.clone(),
        concurrent_barrier,
    );
    let (first_started, second_started) = tokio::join!(first_start, second_start);
    assert_eq!(
        usize::from(first_started?) + usize::from(second_started?),
        1
    );

    let second_active_target = sqlx::query(
        "INSERT INTO tracking_sessions (id, tenant_id, target_id, device_id) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, $3::uuid)",
    )
    .bind(&tenant)
    .bind(&target)
    .bind(&other_device)
    .execute(&pool)
    .await
    .expect_err("target can have only one active session");
    assert_sqlstate(&second_active_target, "23505");

    let second_active_device = sqlx::query(
        "INSERT INTO tracking_sessions (id, tenant_id, target_id, device_id) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, $3::uuid)",
    )
    .bind(&tenant)
    .bind(&other_target)
    .bind(&device)
    .execute(&pool)
    .await
    .expect_err("device can have only one active session");
    let active_device_state = second_active_device
        .as_database_error()
        .and_then(|db| db.code());
    assert!(
        matches!(active_device_state.as_deref(), Some("23505" | "23P01")),
        "active device conflict must be rejected by a uniqueness or overlap constraint: {second_active_device}"
    );

    insert_session(
        &pool,
        &tenant,
        &target,
        &other_device,
        "2024-01-01T10:00:00Z",
        Some("2024-01-01T11:00:00Z"),
    )
    .await?;
    let overlap = insert_session(
        &pool,
        &tenant,
        &other_target,
        &other_device,
        "2024-01-01T10:30:00Z",
        Some("2024-01-01T11:30:00Z"),
    )
    .await
    .expect_err("historical intervals for one device cannot overlap");
    assert_sqlstate(&overlap, "23P01");
    insert_session(
        &pool,
        &tenant,
        &other_target,
        &other_device,
        "2024-01-01T11:00:00Z",
        Some("2024-01-01T12:00:00Z"),
    )
    .await?;
    insert_session(
        &pool,
        &tenant,
        &other_target,
        &other_device,
        "2024-01-01T12:00:00Z",
        Some("2024-01-01T12:00:00Z"),
    )
    .await?;
    let zero_duration_is_empty: bool = sqlx::query_scalar(
        "SELECT isempty(session_period) FROM tracking_sessions \
         WHERE tenant_id=$1::uuid AND device_id=$2::uuid \
           AND started_at='2024-01-01T12:00:00Z'::timestamptz \
           AND ended_at=started_at",
    )
    .bind(&tenant)
    .bind(&other_device)
    .fetch_one(&pool)
    .await?;
    assert!(
        zero_duration_is_empty,
        "[start,end) permits an empty zero-duration interval"
    );

    let expires_utc: String = sqlx::query_scalar(
        "SELECT to_char(position_expires_at_utc('2024-02-29T12:34:56Z'::timestamptz) AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(expires_utc, "2025-02-28 12:34:56");
    let expires_60_days: String = sqlx::query_scalar(
        "SELECT to_char(position_expires_at_utc_days('2024-02-29T12:34:56Z'::timestamptz, 60) AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(expires_60_days, "2024-04-29 12:34:56");

    let message_id: String = sqlx::query_scalar("SELECT gen_random_uuid()::text")
        .fetch_one(&pool)
        .await?;
    let fingerprint: Vec<u8> = sqlx::query_scalar(
        "SELECT decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex')",
    )
    .fetch_one(&pool)
    .await?;
    let (position_id, stored_expiry): (i64, String) = sqlx::query_as(
        "INSERT INTO positions (tenant_id, device_id, message_id, payload_fingerprint, location, recorded_at) VALUES ($1::uuid, $2::uuid, $3::text, $4, ST_SetSRID(ST_MakePoint(110.84, -6.8), 4326)::geography, '2024-02-29T12:34:56Z'::timestamptz) RETURNING id, to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI:SS')",
    )
    .bind(&tenant)
    .bind(&device)
    .bind(&message_id)
    .bind(&fingerprint)
    .fetch_one(&pool)
    .await?;
    assert_eq!(stored_expiry, "2025-02-28 12:34:56");
    let point: (f64, f64) = sqlx::query_as(
        "SELECT ST_X(location::geometry), ST_Y(location::geometry) FROM positions WHERE id = $1",
    )
    .bind(position_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        point,
        (110.84, -6.8),
        "coordinates use longitude, latitude order"
    );

    let duplicate_position = sqlx::query(
        "INSERT INTO positions (tenant_id, device_id, message_id, payload_fingerprint, location, recorded_at) VALUES ($1::uuid, $2::uuid, $3::text, $4, ST_SetSRID(ST_MakePoint(110.84, -6.8), 4326)::geography, '2024-02-29T12:34:56Z'::timestamptz)",
    )
    .bind(&tenant)
    .bind(&device)
    .bind(&message_id)
    .bind(&fingerprint)
    .execute(&pool)
    .await
    .expect_err("device message IDs are deduplicated by unique constraint");
    assert_sqlstate(&duplicate_position, "23505");

    sqlx::query(
        "INSERT INTO idempotency_records (tenant_id, principal_id, route_key, idempotency_key, request_hash, response, status_code, expires_at) VALUES ($1::uuid, $2::uuid, '/v1/positions', 'retry-1', decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), '{}'::jsonb, 201, clock_timestamp() + interval '12 hours')",
    )
    .bind(&tenant)
    .bind(&user_id)
    .execute(&pool)
    .await?;
    let duplicate_idempotency = sqlx::query(
        "INSERT INTO idempotency_records (tenant_id, principal_id, route_key, idempotency_key, request_hash, response, status_code, expires_at) VALUES ($1::uuid, $2::uuid, '/v1/positions', 'retry-1', decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), '{}'::jsonb, 201, clock_timestamp() + interval '12 hours')",
    )
    .bind(&tenant)
    .bind(&user_id)
    .execute(&pool)
    .await
    .expect_err("idempotency scope is unique per tenant/principal/route/key");
    assert_sqlstate(&duplicate_idempotency, "23505");

    sqlx::query(
        "INSERT INTO audit_events (tenant_id, actor_ref, action, outcome, request_id) VALUES ($1::uuid, 'user:test', 'position.read', 'success', 'req-db02')",
    )
    .bind(&tenant)
    .execute(&pool)
    .await?;

    let cross_tenant_position = sqlx::query(
        "INSERT INTO positions (tenant_id, device_id, payload_fingerprint, location, recorded_at) VALUES ($1::uuid, $2::uuid, decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), ST_SetSRID(ST_MakePoint(0, 0), 4326)::geography, clock_timestamp())",
    )
    .bind(&other_tenant)
    .bind(&device)
    .execute(&pool)
    .await
    .expect_err("positions must belong to their device tenant");
    assert_sqlstate(&cross_tenant_position, "23503");

    let expired_position_id: i64 = sqlx::query_scalar(
        "INSERT INTO positions (tenant_id, device_id, message_id, payload_fingerprint, location, recorded_at) VALUES ($1::uuid, $2::uuid, gen_random_uuid()::text, decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), ST_SetSRID(ST_MakePoint(110.84, -6.8), 4326)::geography, '2020-01-01T00:00:00Z'::timestamptz) RETURNING id",
    )
    .bind(&tenant)
    .bind(&device)
    .fetch_one(&pool)
    .await?;
    sqlx::query("UPDATE audit_events SET occurred_at = '2020-01-01T00:00:00Z' WHERE request_id = 'req-db02'")
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE idempotency_records SET created_at = '2019-12-31T12:00:00Z', expires_at = '2020-01-01T00:00:00Z' WHERE idempotency_key = 'retry-1'")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO auth_sessions (id, user_id, secret_hash, csrf_hash, auth_version, created_at, last_interactive_at, expires_at, revoked_at) VALUES (gen_random_uuid(), $1::uuid, decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), 0, '2019-12-01Z', '2019-12-01Z', '2019-12-02Z', '2019-12-01Z')")
        .bind(&user_id)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO auth_challenges (id, user_id, secret_hash, purpose, expires_at, created_at, consumed_at) VALUES (gen_random_uuid(), $1::uuid, decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), 'mfa_login', '2019-12-02Z', '2019-12-01Z', '2019-12-01Z')")
        .bind(&user_id)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO account_tokens (id, user_id, purpose, secret_hash, expires_at, created_at, consumed_at) VALUES (gen_random_uuid(), $1::uuid, 'password_reset', decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), '2019-12-02Z', '2019-12-01Z', '2019-12-01Z')")
        .bind(&user_id)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO recovery_codes (id, user_id, code_hash, created_at, consumed_at) VALUES (gen_random_uuid(), $1::uuid, decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), '2019-12-01Z', '2019-12-02Z')")
        .bind(&user_id)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO email_outbox (id, tenant_id, recipient, template, encrypted_payload, status, created_at, sent_at, retention_expires_at) VALUES (gen_random_uuid(), NULL, 'fixture@example.invalid', 'security_notice', decode('00', 'hex'), 'sent', '2019-12-01Z', '2019-12-01Z', '2019-12-02Z')")
        .execute(&pool)
        .await?;
    let purge = purge_expired_data(&pool, 60).await?;
    assert!(purge.positions >= 1);
    assert!(purge.audit_events >= 1);
    assert!(
        purge.idempotency_records >= 1,
        "expired retry records are deleted after their retry window"
    );
    assert!(purge.email_outbox >= 1);
    assert!(purge.auth_sessions >= 1);
    assert!(purge.auth_challenges >= 1);
    assert!(purge.account_tokens >= 1);
    assert!(purge.recovery_codes >= 1);
    let position_remains: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM positions WHERE id = $1)")
            .bind(expired_position_id)
            .fetch_one(&pool)
            .await?;
    assert!(!position_remains, "expired location fixture is purged");

    let lock_barrier = std::sync::Arc::new(Barrier::new(2));
    let reverse_a = lock_resources_and_commit(
        pool.clone(),
        &tenant,
        vec![device.clone(), other_device.clone()],
        vec![target.clone(), other_target.clone()],
        lock_barrier.clone(),
    );
    let reverse_b = lock_resources_and_commit(
        pool.clone(),
        &tenant,
        vec![other_device, device],
        vec![other_target, target],
        lock_barrier,
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        let (a, b) = tokio::join!(reverse_a, reverse_b);
        a?;
        b?;
        Ok::<(), sqlx::Error>(())
    })
    .await??;

    pool.close().await;
    Ok(())
}

async fn create_tenant(pool: &PgPool, name: &str) -> Result<String, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO organizations (id, name) VALUES (gen_random_uuid(), $1) RETURNING id::text",
    )
    .bind(name)
    .fetch_one(pool)
    .await
}

async fn create_target(
    pool: &PgPool,
    tenant: &str,
    name: &str,
    external_ref: Option<&str>,
) -> Result<String, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO targets (id, tenant_id, name, external_ref) VALUES (gen_random_uuid(), $1::uuid, $2, $3) RETURNING id::text",
    )
    .bind(tenant)
    .bind(name)
    .bind(external_ref)
    .fetch_one(pool)
    .await
}

async fn create_device(
    pool: &PgPool,
    tenant: &str,
    name: &str,
    protocol: &str,
    identifier: &str,
) -> Result<String, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO devices (id, tenant_id, name, protocol, identifier) VALUES (gen_random_uuid(), $1::uuid, $2, $3, $4) RETURNING id::text",
    )
    .bind(tenant)
    .bind(name)
    .bind(protocol)
    .bind(identifier)
    .fetch_one(pool)
    .await
}

async fn seed_position(
    pool: &PgPool,
    tenant: &str,
    device: &str,
    recorded_at: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO positions (tenant_id, device_id, payload_fingerprint, location, recorded_at) \
         VALUES ($1::uuid, $2::uuid, decode(md5(gen_random_uuid()::text) || md5(gen_random_uuid()::text), 'hex'), \
                 ST_SetSRID(ST_MakePoint(110.84, -6.8), 4326)::geography, $3::timestamptz) RETURNING id",
    )
    .bind(tenant)
    .bind(device)
    .bind(recorded_at)
    .fetch_one(pool)
    .await
}

async fn insert_session(
    pool: &PgPool,
    tenant: &str,
    target: &str,
    device: &str,
    start: &str,
    end: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tracking_sessions (id, tenant_id, target_id, device_id, started_at, ended_at) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, $3::uuid, $4::timestamptz, $5::timestamptz)",
    )
    .bind(tenant)
    .bind(target)
    .bind(device)
    .bind(start)
    .bind(end)
    .execute(pool)
    .await?;
    Ok(())
}

async fn attempt_session_start(
    pool: PgPool,
    tenant: String,
    device: String,
    target: String,
    barrier: std::sync::Arc<Barrier>,
) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    barrier.wait().await;
    lock_tracking_resources(
        &mut transaction,
        &tenant,
        std::slice::from_ref(&device),
        std::slice::from_ref(&target),
        &[],
    )
    .await?;
    match sqlx::query(
        "INSERT INTO tracking_sessions (id, tenant_id, target_id, device_id) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, $3::uuid)",
    )
    .bind(&tenant)
    .bind(target)
    .bind(device)
    .execute(&mut *transaction)
    .await
    {
        Ok(_) => {
            transaction.commit().await?;
            Ok(true)
        }
        Err(error) if matches!(error.as_database_error().and_then(|db| db.code()).as_deref(), Some("23505" | "23P01")) => {
            transaction.rollback().await?;
            Ok(false)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

async fn lock_resources_and_commit(
    pool: PgPool,
    tenant: &str,
    devices: Vec<String>,
    targets: Vec<String>,
    barrier: std::sync::Arc<Barrier>,
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    barrier.wait().await;
    lock_tracking_resources(&mut transaction, tenant, &devices, &targets, &[]).await?;
    transaction.commit().await
}

fn assert_sqlstate(error: &sqlx::Error, expected: &str) {
    assert_eq!(
        error
            .as_database_error()
            .and_then(|db| db.code())
            .as_deref(),
        Some(expected),
        "unexpected database error: {error}"
    );
}
