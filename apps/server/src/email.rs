use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType,
    transport::smtp::authentication::Credentials,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{Secret, mfa};

pub async fn start_from_env(
    origin: String,
) -> Result<Option<tokio::task::JoinHandle<()>>, &'static str> {
    let Ok(database_url) = env::var("MAILER_DATABASE_URL") else {
        return Ok(None);
    };
    if database_url.trim().is_empty() {
        return Ok(None);
    }
    let key = env::var("OUTBOX_ENCRYPTION_KEY")
        .ok()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .and_then(|value| <[u8; 32]>::try_from(value).ok())
        .ok_or("OUTBOX_ENCRYPTION_KEY must be 32-byte URL-safe Base64")?;
    let smtp =
        SmtpConfig::from_env()?.ok_or("SMTP_HOST is required when MAILER_DATABASE_URL is set")?;
    smtp.from
        .parse::<lettre::message::Mailbox>()
        .map_err(|_| "SMTP_FROM must be a valid mailbox")?;
    smtp.transport()?;
    let pool = crate::database_pool_options()
        .connect(&database_url)
        .await
        .map_err(|_| "mailer database connection failed")?;
    Ok(Some(tokio::spawn(run(pool, key, origin, smtp))))
}

#[derive(Clone, Copy)]
enum TlsMode {
    Implicit,
    StartTls,
    LocalPlain,
}

#[derive(Clone)]
pub struct SmtpConfig {
    host: String,
    port: u16,
    from: String,
    username: Option<String>,
    password: Option<Secret>,
    tls: TlsMode,
}

impl SmtpConfig {
    pub fn from_env() -> Result<Option<Self>, &'static str> {
        let Ok(host) = env::var("SMTP_HOST") else {
            return Ok(None);
        };
        let port = env::var("SMTP_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1025);
        let tls = match env::var("SMTP_TLS").as_deref().unwrap_or("none") {
            "tls" => TlsMode::Implicit,
            "starttls" => TlsMode::StartTls,
            "none"
                if host == "localhost"
                    || host == "127.0.0.1"
                    || host == "::1"
                    || host == "mailpit" =>
            {
                TlsMode::LocalPlain
            }
            "none" => return Err("plaintext SMTP is restricted to loopback or local Mailpit"),
            _ => return Err("SMTP_TLS must be none, starttls, or tls"),
        };
        let username = env::var("SMTP_USERNAME")
            .ok()
            .filter(|value| !value.is_empty());
        let password = env::var("SMTP_PASSWORD")
            .ok()
            .filter(|value| !value.is_empty())
            .map(Secret);
        if username.is_some() != password.is_some() {
            return Err("SMTP username/password must be configured together");
        }
        if matches!(tls, TlsMode::LocalPlain) && username.is_some() {
            return Err("SMTP credentials require TLS");
        }
        if !matches!(tls, TlsMode::LocalPlain) && username.is_none() {
            return Err("SMTP credentials are required for non-local relays");
        }
        let from = env::var("SMTP_FROM")
            .unwrap_or_else(|_| "Backend Tracker <no-reply@localhost>".to_owned());
        Ok(Some(Self {
            host,
            port,
            from,
            username,
            password,
            tls,
        }))
    }

    fn transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>, &'static str> {
        let builder = match self.tls {
            TlsMode::Implicit => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host),
            TlsMode::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host),
            TlsMode::LocalPlain => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                &self.host,
            )),
        }
        .map_err(|_| "SMTP transport configuration failed")?;
        let mut builder = builder.port(self.port);
        if let (Some(username), Some(password)) = (&self.username, &self.password) {
            builder = builder.credentials(Credentials::new(
                username.clone(),
                password.expose().to_owned(),
            ));
        }
        Ok(builder.build())
    }
}

#[derive(Deserialize, Serialize)]
pub struct Payload {
    pub token: String,
    pub expires_at: u64,
}

pub fn encrypt_payload(key: &[u8; 32], token: &str, expires_at: u64) -> Option<Vec<u8>> {
    let payload = serde_json::to_vec(&Payload {
        token: token.to_owned(),
        expires_at,
    })
    .ok()?;
    let (ciphertext, nonce) = mfa::encrypt(key, &payload)?;
    let mut stored = nonce;
    stored.extend(ciphertext);
    Some(stored)
}

pub async fn run(pool: PgPool, key: [u8; 32], origin: String, config: SmtpConfig) {
    let transport = match config.transport() {
        Ok(value) => value,
        Err(_) => return,
    };
    let from = match config.from.parse::<lettre::message::Mailbox>() {
        Ok(value) => value,
        Err(_) => return,
    };
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        interval.tick().await;
        match send_one(&pool, &key, &origin, &from, &transport).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(_) => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
        }
    }
}

async fn send_one(
    pool: &PgPool,
    key: &[u8; 32],
    origin: &str,
    from: &lettre::message::Mailbox,
    transport: &AsyncSmtpTransport<Tokio1Executor>,
) -> Result<bool, sqlx::Error> {
    sqlx::query("DELETE FROM email_outbox WHERE retention_expires_at<=clock_timestamp()")
        .execute(pool)
        .await?;
    let mut tx = pool.begin().await?;
    let row = sqlx::query_as::<_, (String, String, String, Vec<u8>, i16)>(
        "SELECT id::text,recipient,template,encrypted_payload,attempts FROM email_outbox \
         WHERE retention_expires_at>clock_timestamp() AND next_attempt_at<=clock_timestamp() \
           AND (status IN ('pending','failed') OR status='sending') AND attempts<20 \
         ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, recipient, template, encrypted, attempts)) = row else {
        tx.rollback().await?;
        return Ok(false);
    };
    sqlx::query("UPDATE email_outbox SET status='sending',attempts=attempts+1,next_attempt_at=clock_timestamp()+interval '2 minutes',last_error_code=NULL WHERE id=$1::uuid")
        .bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;

    let result = match (encrypted.len() >= 12)
        .then(|| mfa::decrypt(key, &encrypted[12..], &encrypted[..12]))
        .flatten()
        .and_then(|plain| serde_json::from_slice::<Payload>(&plain).ok())
    {
        Some(payload) => deliver(transport, from, &recipient, &template, payload, origin).await,
        None => Err("invalid_message"),
    };
    match result {
        Ok(()) => {
            sqlx::query("DELETE FROM email_outbox WHERE id=$1::uuid")
                .bind(id)
                .execute(pool)
                .await?;
        }
        Err(code) => {
            let backoff = 2_i64.pow((attempts as u32).min(8)).min(3600);
            let retry = code == "smtp_unavailable" && attempts < 19;
            let last_error = if code == "expired" {
                "expired"
            } else if code == "invalid_message" {
                "invalid_message"
            } else if retry {
                "smtp_unavailable"
            } else {
                "attempts_exhausted"
            };
            sqlx::query("UPDATE email_outbox SET status='failed',next_attempt_at=CASE WHEN $2 THEN clock_timestamp()+make_interval(secs=>$3) ELSE clock_timestamp()+interval '365 days' END,last_error_code=$4 WHERE id=$1::uuid")
                .bind(id).bind(retry).bind(backoff as i32).bind(last_error).execute(pool).await?;
        }
    }
    Ok(true)
}

async fn deliver(
    transport: &AsyncSmtpTransport<Tokio1Executor>,
    from: &lettre::message::Mailbox,
    recipient: &str,
    template: &str,
    payload: Payload,
    origin: &str,
) -> Result<(), &'static str> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "invalid_message")?
        .as_secs();
    if payload.expires_at <= now {
        return Err("expired");
    }
    let email_to = recipient.parse().map_err(|_| "invalid_message")?;
    let path = match template {
        "password_reset" => "reset-password",
        "account_activation" => "activate",
        "account_recovery" => "recover",
        "invitation" => "accept-invitation",
        _ => return Err("invalid_message"),
    };
    let fragment_key = if template == "account_recovery" {
        "email_token"
    } else {
        "token"
    };
    let message = Message::builder().from(from.clone()).to(email_to).subject(match template { "password_reset" => "Reset your password", "account_activation" => "Activate your account", _ => "Account recovery" }).header(ContentType::TEXT_PLAIN).body(format!("Use this one-time link within its expiry window: {origin}/{path}#{fragment_key}={}\n\nIf you did not request this, ignore this message.", payload.token)).map_err(|_| "invalid_message")?;
    transport
        .send(message)
        .await
        .map(|_| ())
        .map_err(|_| "smtp_unavailable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbox_payload_is_authenticated_encryption_and_carries_expiry() {
        let token = "synthetic-one-time-token-that-is-never-logged";
        let key = [23; 32];
        let stored = encrypt_payload(&key, token, 1_800_000_000).unwrap();
        assert!(!String::from_utf8_lossy(&stored).contains(token));
        let decoded = mfa::decrypt(&key, &stored[12..], &stored[..12]).unwrap();
        let payload: Payload = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(payload.token, token);
        assert_eq!(payload.expires_at, 1_800_000_000);
        let mut tampered = stored;
        tampered[15] ^= 0x80;
        assert!(mfa::decrypt(&key, &tampered[12..], &tampered[..12]).is_none());
    }
}
