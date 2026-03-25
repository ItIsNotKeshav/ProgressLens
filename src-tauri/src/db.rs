// ──────────────────────────────────────────────────────────────
// db.rs — database initialisation and migration runner
// ──────────────────────────────────────────────────────────────
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::path::PathBuf;
use std::str::FromStr;

/// Open (or create) the SQLite database and run all pending migrations.
pub async fn init_db(db_path: PathBuf) -> Result<SqlitePool, sqlx::Error> {
    // Make sure the parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| sqlx::Error::Io(e))?;
    }

    let db_url = format!("sqlite://{}", db_path.display());

    let opts = SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePool::connect_with(opts).await?;

    // Run embedded migrations from migrations/ directory
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Cleanup: remove rogue empty fields (from previous ranges=A:ZZ bug)
    sqlx::query("DELETE FROM fields WHERE sheet_key = ''")
        .execute(&pool)
        .await
        .ok(); // Ignore if it fails

    Ok(pool)
}
