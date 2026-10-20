use sqlx::PgPool;
use std::{env, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("migration: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), &'static str> {
    let url = env::var("MIGRATOR_DATABASE_URL").map_err(|_| "MIGRATOR_DATABASE_URL is required")?;
    let pool = PgPool::connect(&url)
        .await
        .map_err(|_| "database connection failed")?;
    tracking_storage::migrate(&pool)
        .await
        .map_err(|_| "migration failed; inspect the database migration state")?;
    println!("database migrations are up to date");
    Ok(())
}
