use anyhow::{Result, Context};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use std::path::Path;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectInfo {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub last_active: i64,
}

pub struct StasherHub {
    pub pool: SqlitePool,
}

impl StasherHub {
    pub async fn init() -> Result<Self> {
        let home_dir = home::home_dir().context("Could not find home directory")?;
        let hub_dir = home_dir.join(".stasher");
        
        if !hub_dir.exists() {
            std::fs::create_dir_all(&hub_dir)?;
        }

        let db_path = hub_dir.join("hub.db");
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(options).await?;

        // Create the projects table if it doesn't exist
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                last_active INTEGER NOT NULL
            )"
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    pub async fn register_project(&self, project_path: &Path) -> Result<()> {
        let path_str = project_path.to_string_lossy().to_string();
        let name = project_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO projects (name, path, last_active) 
             VALUES (?, ?, ?)
             ON CONFLICT(path) DO UPDATE SET last_active = excluded.last_active"
        )
        .bind(name)
        .bind(path_str)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectInfo>> {
        let projects = sqlx::query_as::<_, ProjectInfo>(
            "SELECT id, name, path, last_active FROM projects ORDER BY last_active DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(projects)
    }

    /// Aggregate tag usage across all registered projects.
    pub async fn get_tag_analytics(&self) -> Result<std::collections::HashMap<String, u64>> {
        use std::collections::HashMap;

        let projects = self.list_projects().await?;
        let mut tag_counts: HashMap<String, u64> = HashMap::new();

        for project in projects {
            let project_path = std::path::PathBuf::from(&project.path);
            if !project_path.exists() {
                continue;
            }

            let stasher_db = project_path.join(".stasher/metadata.db");
            if !stasher_db.exists() {
                continue;
            }

            let options = SqliteConnectOptions::new()
                .filename(&stasher_db)
                .create_if_missing(false);

            if let Ok(project_pool) = SqlitePool::connect_with(options).await {
                let rows: Vec<(String,)> = sqlx::query_as(
                    "SELECT COALESCE(tags, '') FROM sessions WHERE tags IS NOT NULL AND tags != ''"
                )
                .fetch_all(&project_pool)
                .await
                .unwrap_or_default();

                for (tags_str,) in rows {
                    for tag in tags_str.split(',').filter(|t| !t.is_empty()) {
                        *tag_counts.entry(tag.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }

        Ok(tag_counts)
    }
}
