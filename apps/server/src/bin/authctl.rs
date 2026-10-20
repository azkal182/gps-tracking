use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use server::{email, hash_password};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{collections::HashMap, env, fs::OpenOptions, io::Write, path::Path, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("authctl: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), &'static str> {
    let mut args = env::args().skip(1);
    let command = args
        .next()
        .ok_or("expected bootstrap-admin or operator-recovery")?;
    let mut flags = HashMap::new();
    while let Some(flag) = args.next() {
        if !flag.starts_with("--") {
            return Err("expected named options");
        }
        let value = args.next().ok_or("option requires a value")?;
        if flags.insert(flag, value).is_some() {
            return Err("duplicate option");
        }
    }
    let database_url =
        env::var("AUTH_DATABASE_URL").map_err(|_| "AUTH_DATABASE_URL is required")?;
    let key = env::var("OUTBOX_ENCRYPTION_KEY")
        .ok()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .and_then(|value| <[u8; 32]>::try_from(value).ok())
        .ok_or("OUTBOX_ENCRYPTION_KEY must be 32-byte URL-safe Base64")?;
    let pool = PgPool::connect(&database_url)
        .await
        .map_err(|_| "database connection failed")?;
    match command.as_str() {
        "bootstrap-admin" => bootstrap(&pool, &key, &flags).await,
        "resend-activation" => resend_activation(&pool, &key, &flags).await,
        "operator-recovery" => operator_recovery(&pool, &key, &flags).await,
        _ => Err("expected bootstrap-admin, resend-activation or operator-recovery"),
    }
}

async fn bootstrap(
    pool: &PgPool,
    key: &[u8; 32],
    flags: &HashMap<String, String>,
) -> Result<(), &'static str> {
    let email = required(flags, "--email")?.trim().to_ascii_lowercase();
    let display_name = required(flags, "--display-name")?.trim();
    let reason = required(flags, "--reason")?.trim();
    let case_id = required(flags, "--case-id")?.trim();
    let operator = required(flags, "--operator")?.trim();
    validate_common(&email, reason, case_id, operator)?;
    if reason.len() > 90 {
        return Err("bootstrap reason must be at most 90 bytes");
    }
    if display_name.is_empty()
        || display_name.len() > 150
        || email.parse::<lettre::Address>().is_err()
    {
        return Err("email or display name is invalid");
    }

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "database operation failed")?;
    sqlx::query("SELECT pg_advisory_xact_lock(722301904031::bigint)")
        .execute(&mut *tx)
        .await
        .map_err(|_| "database operation failed")?;
    let exists: bool = sqlx::query_scalar("SELECT public.gps_tracking_platform_admin_exists()")
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| "database operation failed")?;
    if exists {
        return Err("platform admin bootstrap has already been completed");
    }

    let user_id = uuid::Uuid::new_v4().to_string();
    let unusable_secret = random_secret();
    let password_hash =
        hash_password(unusable_secret.as_bytes()).map_err(|_| "password hashing failed")?;
    drop(unusable_secret);
    sqlx::query("INSERT INTO users(id,email_canonical,display_name,password_hash,status,platform_admin) VALUES($1::uuid,$2,$3,$4,'pending_activation',TRUE)")
        .bind(&user_id).bind(&email).bind(display_name).bind(password_hash).execute(&mut *tx).await.map_err(|_| "bootstrap account could not be created")?;
    let token = random_secret();
    let token_id = uuid::Uuid::new_v4().to_string();
    let expires_at: i64 = sqlx::query_scalar("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'activation',$3,clock_timestamp()+interval '72 hours') RETURNING extract(epoch FROM expires_at)::bigint")
        .bind(&token_id).bind(&user_id).bind(digest(&token)).fetch_one(&mut *tx).await.map_err(|_| "activation token could not be created")?;
    let encrypted =
        email::encrypt_payload(key, &token, expires_at as u64).ok_or("outbox encryption failed")?;
    sqlx::query("INSERT INTO email_outbox(id,recipient,template,encrypted_payload,retention_expires_at) VALUES($1::uuid,$2,'account_activation',$3,clock_timestamp()+interval '72 hours')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&email).bind(encrypted).execute(&mut *tx).await.map_err(|_| "activation email could not be queued")?;
    let action = format!("auth.bootstrap_admin:{reason}");
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,$2,'user',$3::uuid,'success',$4)")
        .bind(operator).bind(action).bind(&user_id).bind(case_id).execute(&mut *tx).await.map_err(|_| "bootstrap audit could not be written")?;
    tx.commit()
        .await
        .map_err(|_| "bootstrap transaction failed")?;
    println!(
        "Bootstrap activation was queued for the specified address; no usable password was created. Case: {case_id}"
    );
    Ok(())
}

async fn resend_activation(
    pool: &PgPool,
    key: &[u8; 32],
    flags: &HashMap<String, String>,
) -> Result<(), &'static str> {
    let email = required(flags, "--email")?.trim().to_ascii_lowercase();
    let reason = required(flags, "--reason")?.trim();
    let case_id = required(flags, "--case-id")?.trim();
    let operator = required(flags, "--operator")?.trim();
    validate_common(&email, reason, case_id, operator)?;
    if reason.len() > 100 {
        return Err("activation reason must be at most 100 bytes");
    }
    if email.parse::<lettre::Address>().is_err() {
        return Err("email is invalid");
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "database operation failed")?;
    let user = sqlx::query_as::<_, (String, String)>("SELECT id::text,email_canonical FROM users WHERE email_canonical=$1 AND status='pending_activation' FOR UPDATE")
        .bind(&email).fetch_optional(&mut *tx).await.map_err(|_| "database operation failed")?.ok_or("pending bootstrap account was not found")?;
    sqlx::query("UPDATE account_tokens SET revoked_at=clock_timestamp() WHERE user_id=$1::uuid AND purpose='activation' AND consumed_at IS NULL AND revoked_at IS NULL")
        .bind(&user.0).execute(&mut *tx).await.map_err(|_| "old activation link could not be revoked")?;
    let token = random_secret();
    let expires_at: i64 = sqlx::query_scalar("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'activation',$3,clock_timestamp()+interval '72 hours') RETURNING extract(epoch FROM expires_at)::bigint")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&user.0).bind(digest(&token)).fetch_one(&mut *tx).await.map_err(|_| "activation token could not be created")?;
    let encrypted =
        email::encrypt_payload(key, &token, expires_at as u64).ok_or("outbox encryption failed")?;
    sqlx::query("INSERT INTO email_outbox(id,recipient,template,encrypted_payload,retention_expires_at) VALUES($1::uuid,$2,'account_activation',$3,clock_timestamp()+interval '72 hours')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&user.1).bind(encrypted).execute(&mut *tx).await.map_err(|_| "activation email could not be queued")?;
    let action = format!("auth.resend_activation:{reason}");
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,$2,'user',$3::uuid,'success',$4)")
        .bind(operator).bind(action).bind(&user.0).bind(case_id).execute(&mut *tx).await.map_err(|_| "activation audit could not be written")?;
    tx.commit()
        .await
        .map_err(|_| "activation transaction failed")?;
    println!(
        "A new activation email was queued; previous activation links were revoked. Case: {case_id}"
    );
    Ok(())
}

async fn operator_recovery(
    pool: &PgPool,
    key: &[u8; 32],
    flags: &HashMap<String, String>,
) -> Result<(), &'static str> {
    let email = required(flags, "--email")?.trim().to_ascii_lowercase();
    let reason = required(flags, "--reason")?.trim();
    let case_id = required(flags, "--case-id")?.trim();
    let operator = required(flags, "--operator")?.trim();
    validate_common(&email, reason, case_id, operator)?;
    if email.parse::<lettre::Address>().is_err() {
        return Err("email is invalid");
    }
    let token_path = required(flags, "--token-file")?;
    let authorization = random_secret();
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "database operation failed")?;
    let user = sqlx::query_as::<_, (String, String)>("SELECT id::text,email_canonical FROM users WHERE email_canonical=$1 AND status='active' FOR UPDATE")
        .bind(&email).fetch_optional(&mut *tx).await.map_err(|_| "database operation failed")?.ok_or("active account was not found")?;
    let token_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'recovery',$3,clock_timestamp()+interval '15 minutes')")
        .bind(&token_id).bind(&user.0).bind(digest(&authorization)).execute(&mut *tx).await.map_err(|_| "recovery authorization could not be created")?;
    let proof = random_secret();
    let proof_expires: i64 = sqlx::query_scalar("INSERT INTO account_tokens(id,user_id,purpose,secret_hash,expires_at) VALUES($1::uuid,$2::uuid,'recovery_email',$3,clock_timestamp()+interval '15 minutes') RETURNING extract(epoch FROM expires_at)::bigint")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&user.0).bind(digest(&proof)).fetch_one(&mut *tx).await.map_err(|_| "email proof could not be created")?;
    let encrypted = email::encrypt_payload(key, &proof, proof_expires as u64)
        .ok_or("outbox encryption failed")?;
    sqlx::query("INSERT INTO email_outbox(id,recipient,template,encrypted_payload,retention_expires_at) VALUES($1::uuid,$2,'account_recovery',$3,clock_timestamp()+interval '72 hours')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&user.1).bind(encrypted).execute(&mut *tx).await.map_err(|_| "recovery email could not be queued")?;
    sqlx::query("INSERT INTO operator_recovery_cases(case_id,user_id,operator_ref,reason,recovery_token_id) VALUES($1,$2::uuid,$3,$4,$5::uuid)")
        .bind(case_id).bind(&user.0).bind(operator).bind(reason).bind(&token_id).execute(&mut *tx).await.map_err(|_| "case ID already exists or audit case could not be written")?;
    sqlx::query("INSERT INTO audit_events(tenant_id,actor_ref,action,resource_type,resource_id,outcome,request_id) VALUES(NULL,$1,'auth.operator_recovery_authorized','user',$2::uuid,'success',$3)")
        .bind(operator).bind(&user.0).bind(case_id).execute(&mut *tx).await.map_err(|_| "recovery audit could not be written")?;
    write_secret_file(Path::new(token_path), &authorization)?;
    if tx.commit().await.is_err() {
        let _ = std::fs::remove_file(token_path);
        return Err("recovery transaction failed");
    }
    println!(
        "Recovery authorization stored as a digest and written to the requested mode-0600 file. Share it through the verified channel, then securely remove the file. Case: {case_id}; authorization expires in 15 minutes."
    );
    Ok(())
}

#[cfg(unix)]
fn write_secret_file(path: &Path, secret: &str) -> Result<(), &'static str> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| "token file must be a new path writable by this operator")?;
    file.write_all(secret.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|_| "could not safely write token file")
}

#[cfg(not(unix))]
fn write_secret_file(_path: &Path, _secret: &str) -> Result<(), &'static str> {
    Err("operator recovery token file requires Unix permissions mode 0600")
}

fn required<'a>(flags: &'a HashMap<String, String>, key: &str) -> Result<&'a str, &'static str> {
    flags
        .get(key)
        .map(String::as_str)
        .ok_or("required option is missing")
}
fn validate_common(
    email: &str,
    reason: &str,
    case_id: &str,
    operator: &str,
) -> Result<(), &'static str> {
    if email.is_empty()
        || reason.len() < 10
        || reason.len() > 1000
        || case_id.is_empty()
        || case_id.len() > 128
        || operator.is_empty()
        || operator.len() > 160
    {
        return Err("email, reason, case ID or operator value is invalid");
    }
    Ok(())
}
fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn digest(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}
