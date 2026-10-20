use super::*;

mod auth;
mod devices;
mod live;
mod organizations;
mod tracking;
mod unit;

pub(super) async fn insert_org01_user(pool: &PgPool, email: &str, platform: bool) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users(id,email_canonical,display_name,password_hash,status,platform_admin) VALUES($1::uuid,$2,'ORG-01 Fixture',$3,'active',$4)").bind(&id).bind(email).bind("$argon2id$v=19$m=65536,t=3,p=1$synthetic$synthetic-fixture-only").bind(platform).execute(pool).await.unwrap();
    id
}

pub(super) async fn insert_org01_session(
    pool: &PgPool,
    user_id: &str,
    reauthenticated: bool,
) -> (String, String) {
    let secret = random_secret();
    let csrf = random_secret();
    sqlx::query("INSERT INTO auth_sessions(id,user_id,secret_hash,csrf_hash,auth_version,expires_at,reauthenticated_at) VALUES($1::uuid,$2::uuid,$3,$4,0,clock_timestamp()+interval '12 hours',CASE WHEN $5 THEN clock_timestamp() ELSE NULL END)").bind(uuid::Uuid::new_v4().to_string()).bind(user_id).bind(digest(&secret)).bind(digest(&csrf)).bind(reauthenticated).execute(pool).await.unwrap();
    (secret, csrf)
}
