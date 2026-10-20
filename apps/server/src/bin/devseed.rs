use server::hash_password;
use sqlx::PgPool;
use std::env;
use uuid::Uuid;

const TENANTS: [&str; 2] = [
    "5de80000-0000-4000-8000-000000000001",
    "5de80000-0000-4000-8000-000000000003",
];
const USER: &str = "5de80000-0000-4000-8000-000000000002";
const TARGETS: [&str; 5] = [
    "5de80000-0000-4000-8000-000000000011",
    "5de80000-0000-4000-8000-000000000012",
    "5de80000-0000-4000-8000-000000000013",
    "5de80000-0000-4000-8000-000000000014",
    "5de80000-0000-4000-8000-000000000015",
];
const DEVICES: [&str; 5] = [
    "5de80000-0000-4000-8000-000000000021",
    "5de80000-0000-4000-8000-000000000022",
    "5de80000-0000-4000-8000-000000000023",
    "5de80000-0000-4000-8000-000000000024",
    "5de80000-0000-4000-8000-000000000025",
];
const EMAIL: &str = "demo.admin@local.test";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var("APP_ENV").as_deref() != Ok("development") {
        return Err("devseed requires APP_ENV=development".into());
    }
    let database_url = env::var("GPS_DEV_SEED_DATABASE_URL")
        .map_err(|_| "GPS_DEV_SEED_DATABASE_URL is required")?;
    ensure_local_dev_database(&database_url)?;
    let password = env::var("GPS_DEV_SEED_PASSWORD").map_err(
        |_| "GPS_DEV_SEED_PASSWORD is required; do not pass passwords as command-line arguments",
    )?;
    if password.chars().count() < 15 || password.len() > 128 {
        return Err(
            "GPS_DEV_SEED_PASSWORD must be 15-128 characters to satisfy the login contract".into(),
        );
    }
    let hash = hash_password(password.as_bytes()).map_err(|_| "password hashing failed")?;
    drop(password);

    let pool = PgPool::connect(&database_url).await?;
    tracking_storage::migrate(&pool).await?;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(722301904032::bigint)")
        .execute(&mut *tx)
        .await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email_canonical = $1)")
            .bind(EMAIL)
            .fetch_one(&mut *tx)
            .await?;
    if exists {
        tx.rollback().await?;
        println!("Local demo seed already exists; no records changed.");
        pool.close().await;
        return Ok(());
    }

    for (tenant, name) in TENANTS
        .iter()
        .zip(["Local Demo Organization", "Local Demo Logistics"])
    {
        sqlx::query("INSERT INTO organizations(id,name) VALUES($1::uuid,$2)")
            .bind(tenant)
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("INSERT INTO users(id,email_canonical,display_name,password_hash,status) VALUES($1::uuid,$2,'Local Demo Admin',$3,'active')")
        .bind(USER)
        .bind(EMAIL)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    for tenant in TENANTS {
        sqlx::query("INSERT INTO memberships(id,tenant_id,user_id,role,status) VALUES($1::uuid,$2::uuid,$3::uuid,'organization_admin','active')")
            .bind(Uuid::new_v4().to_string())
            .bind(tenant)
            .bind(USER)
            .execute(&mut *tx)
            .await?;
    }

    let names = [
        "Van Demo 01",
        "Van Demo 02",
        "Motor Demo 03",
        "Truck Demo 04",
        "Van Demo 05",
    ];
    let points = [
        (-6.2088_f64, 106.8456_f64),
        (-6.2110, 106.8501),
        (-6.2043, 106.8390),
        (-7.2575, 112.7521),
        (-7.2600, 112.7580),
    ];
    for i in 0..5 {
        let tenant = TENANTS[usize::from(i >= 3)];
        sqlx::query(
            "INSERT INTO targets(id,tenant_id,name,external_ref) VALUES($1::uuid,$2::uuid,$3,$4)",
        )
        .bind(TARGETS[i])
        .bind(tenant)
        .bind(names[i])
        .bind(format!("DEMO-{}", i + 1))
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO devices(id,tenant_id,name,protocol,identifier,ingestion_policy,last_contact_at) VALUES($1::uuid,$2::uuid,$3,'http-json',$4,'session_only',clock_timestamp()-interval '20 seconds')")
            .bind(DEVICES[i])
            .bind(tenant)
            .bind(format!("Tracker Demo {:02}", i + 1))
            .bind(format!("local-demo-{:02}", i + 1))
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO tracking_sessions(id,tenant_id,target_id,device_id,started_at) VALUES($1::uuid,$2::uuid,$3::uuid,$4::uuid,clock_timestamp()-interval '3 hours')")
            .bind(Uuid::new_v4().to_string())
            .bind(tenant)
            .bind(TARGETS[i])
            .bind(DEVICES[i])
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO positions(tenant_id,device_id,message_id,payload_fingerprint,location,recorded_at) SELECT $1::uuid,$2::uuid,gen_random_uuid(),decode(md5($2::text||g.n::text)||md5(g.n::text||$2::text),'hex'),ST_SetSRID(ST_MakePoint($3 + (g.n * 0.0008),$4 + (g.n * 0.0005)),4326)::geography,clock_timestamp()-(g.n * interval '5 minutes') FROM generate_series(0,23) AS g(n)")
            .bind(tenant)
            .bind(DEVICES[i])
            .bind(points[i].1)
            .bind(points[i].0)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    pool.close().await;
    println!(
        "Local demo seed created: 2 organizations, 1 organization-admin account, 5 targets/devices, 5 active tracking sessions and 120 synthetic positions."
    );
    println!("Login email: {EMAIL}");
    println!("The first browser login must complete the normal MFA enrollment flow.");
    Ok(())
}

fn ensure_local_dev_database(url: &str) -> Result<(), &'static str> {
    let lower = url.to_ascii_lowercase();
    let Some((_, authority_and_path)) = lower.split_once("://") else {
        return Err("seed database URL must use postgres:// or postgresql://");
    };
    let Some((authority, path)) = authority_and_path.rsplit_once('/') else {
        return Err("seed database URL must include a local database name");
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let local_host = host.starts_with("localhost:")
        || host == "localhost"
        || host.starts_with("127.0.0.1:")
        || host == "127.0.0.1"
        || host.starts_with("[::1]:")
        || host == "[::1]";
    let database = path.split(['?', '#']).next().unwrap_or_default();
    if !local_host || !(database.ends_with("_dev") || database.ends_with("_test")) {
        return Err("devseed only accepts localhost databases whose name ends in _dev or _test");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_local_dev_database;

    #[test]
    fn seed_requires_local_dev_or_test_database() {
        assert!(ensure_local_dev_database("postgres://u:p@127.0.0.1:5432/tracker_dev").is_ok());
        assert!(ensure_local_dev_database("postgres://u:p@localhost/tracker_test").is_ok());
        assert!(ensure_local_dev_database("postgres://u:p@db/tracker_dev").is_err());
        assert!(ensure_local_dev_database("postgres://u:p@127.0.0.1/tracker").is_err());
        assert!(ensure_local_dev_database("postgres://u:p@127.0.0.1/tracker_prod").is_err());
    }
}
