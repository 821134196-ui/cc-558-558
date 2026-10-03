use std::str::FromStr;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

pub async fn init_pool(url: &str, max_connections: u32) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .pragma("foreign_keys", "ON");
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
