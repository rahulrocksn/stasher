use anyhow::{Context, Result};
use lancedb::connect;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::path::Path;

pub struct Database {
    pub sqlite: SqlitePool,
    pub lancedb: lancedb::Connection,
}

impl Database {
    pub async fn init(base_path: &Path) -> Result<Self> {
        let stasher_dir = base_path.join(".stasher");
        
        // Create storage dirs
        std::fs::create_dir_all(&stasher_dir)?;
        std::fs::create_dir_all(stasher_dir.join("objects"))?;

        // 1. Initialize SQLite
        let sqlite_path = stasher_dir.join("metadata.db");
        let conn_options = SqliteConnectOptions::new()
            .filename(&sqlite_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let sqlite = SqlitePool::connect_with(conn_options)
            .await
            .context("Failed to connect to SQLite")?;

        // Run migrations
        Self::run_migrations(&sqlite).await?;

        // 2. Initialize LanceDB
        let vector_path = stasher_dir.join("vectors");
        std::fs::create_dir_all(&vector_path)?;
        let abs_path = vector_path.canonicalize()?;
        let lancedb = connect(abs_path.to_str().unwrap()).execute().await?;

        Ok(Self { sqlite, lancedb })
    }

    async fn run_migrations(pool: &SqlitePool) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                start_time INTEGER NOT NULL,
                end_time INTEGER,
                meta TEXT,
                tags TEXT DEFAULT ''
            )"
        ).execute(pool).await?;

        // Migration: add tags column if it doesn't exist (for existing databases)
        let _ = sqlx::query("ALTER TABLE sessions ADD COLUMN tags TEXT DEFAULT ''")
            .execute(pool)
            .await;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS snapshots (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                diff_patch TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                lines_added INTEGER NOT NULL,
                lines_removed INTEGER NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            )"
        ).execute(pool).await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_snapshots_file ON snapshots(file_path)"
        ).execute(pool).await?;

        Ok(())
    }
}
