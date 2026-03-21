use crate::db::Database;
use anyhow::{Context, Result};
use std::path::{PathBuf, Path};
use std::fs;
use chrono::Utc;
use uuid::Uuid;
use similar::{TextDiff, ChangeTag};
use std::sync::Arc;
use regex::Regex;
use crate::search::SearchEngine;

pub struct HistoryManager {
    db: Arc<Database>,
    search: Arc<SearchEngine>,
    base_path: PathBuf,
    objects_path: PathBuf,
    current_session_id: Uuid,
}

impl HistoryManager {
    pub async fn new(db: Arc<Database>, base_path: PathBuf) -> Result<Self> {
        let objects_path = base_path.join(".stasher").join("objects");
        
        // Start a new session for the daemon run
        let session_id = Uuid::new_v4();
        let now = Utc::now().timestamp_millis();
        
        sqlx::query("INSERT INTO sessions (id, start_time) VALUES (?, ?)")
            .bind(session_id.to_string())
            .bind(now)
            .execute(&db.sqlite)
            .await?;

        // Initialize search engine
        let search = Arc::new(SearchEngine::new(db.lancedb.clone()).await?);

        Ok(Self {
            db,
            search,
            base_path,
            objects_path,
            current_session_id: session_id,
        })
    }

    pub fn get_db(&self) -> &Database {
        &self.db
    }

    pub async fn record_change(&self, file_path: PathBuf) -> Result<()> {
        let relative_path = file_path.strip_prefix(&self.base_path)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();

        let content = fs::read_to_string(&file_path)
            .context("Failed to read file for record_change")?;
        
        let new_hash = blake3::hash(content.as_bytes()).to_hex().to_string();

        // Check latest snapshot for this file
        let latest: Option<(String, String)> = sqlx::query_as(
            "SELECT content_hash, diff_patch FROM snapshots WHERE file_path = ? ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(&relative_path)
        .fetch_optional(&self.db.sqlite)
        .await?;

        let (diff_patch, added, removed) = if let Some((old_hash, _)) = latest {
            if old_hash == new_hash {
                // No actual content change
                return Ok(());
            }

            // Generate diff
            let old_content_path = self.objects_path.join(&old_hash);
            let old_content = if old_content_path.exists() {
                fs::read_to_string(old_content_path).unwrap_or_default()
            } else {
                String::new()
            };

            let diff = TextDiff::from_lines(&old_content, &content);
            let mut patch = String::new();
            let mut added = 0;
            let mut removed = 0;

            for hunk in diff.unified_diff().header(&relative_path, &relative_path).iter_hunks() {
                patch.push_str(&format!("{}", hunk));
                for change in hunk.iter_changes() {
                    match change.tag() {
                        ChangeTag::Delete => removed += 1,
                        ChangeTag::Insert => added += 1,
                        _ => {}
                    }
                }
            }
            (patch, added, removed)
        } else {
            // First time seeing this file, diff is the whole file
            let patch = format!("--- /dev/null\n+++ {}\n@@ -0,0 +1,{} @@\n{}", 
                relative_path, 
                content.lines().count(),
                content.lines().map(|l| format!("+{}", l)).collect::<Vec<_>>().join("\n")
            );
            (patch, content.lines().count() as i32, 0)
        };

        let snapshot_id = self.save_snapshot(&relative_path, &new_hash, &diff_patch, added, removed).await?;

        // 3. Index for semantic search
        if let Err(e) = self.search.index_snapshot(snapshot_id, relative_path, content.clone()).await {
            eprintln!("⚠️ Failed to index snapshot: {}", e);
        }

        // Save to CAS
        let object_path = self.objects_path.join(&new_hash);
        fs::write(object_path, content)?;

        Ok(())
    }

    async fn save_snapshot(&self, file_path: &str, hash: &str, patch: &str, added: i32, removed: i32) -> Result<String> {
        let snapshot_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO snapshots (id, session_id, file_path, timestamp, diff_patch, content_hash, lines_added, lines_removed) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&snapshot_id)
        .bind(self.current_session_id.to_string())
        .bind(file_path)
        .bind(now)
        .bind(patch)
        .bind(hash)
        .bind(added)
        .bind(removed)
        .execute(&self.db.sqlite)
        .await?;

        Ok(snapshot_id)
    }

    pub async fn prune_history(&self, days: u32) -> Result<(u64, u64)> {
        let cutoff = Utc::now().timestamp_millis() - (days as i64 * 24 * 60 * 60 * 1000);
        
        // 1. Delete old snapshots from SQLite
        let rows_affected = sqlx::query("DELETE FROM snapshots WHERE timestamp < ?")
            .bind(cutoff)
            .execute(&self.db.sqlite)
            .await?
            .rows_affected();

        // 2. Perform Garbage Collection on the objects folder
        let deleted_objects = self.cleanup_unused_objects().await?;
        
        Ok((rows_affected, deleted_objects))
    }

    async fn cleanup_unused_objects(&self) -> Result<u64> {
        use std::collections::HashSet;
        
        // Find all hashes still referenced in the database
        let active_hashes: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT content_hash FROM snapshots")
            .fetch_all(&self.db.sqlite)
            .await?;
        
        let hash_set: HashSet<String> = active_hashes.into_iter().map(|h| h.0).collect();
        
        let mut deleted_count = 0;
        if self.objects_path.exists() {
            for entry in fs::read_dir(&self.objects_path)? {
                let entry = entry?;
                let file_name = entry.file_name().to_string_lossy().to_string();
                
                // If the file (hash) is not in our active set, it's garbage
                if !hash_set.contains(&file_name) {
                    fs::remove_file(entry.path())?;
                    deleted_count += 1;
                }
            }
        }
        
        Ok(deleted_count)
    }

    pub async fn sync_all(&self) -> Result<()> {
        use ignore::WalkBuilder;
        
        println!("📂 Scanning project for existing files...");
        for entry in WalkBuilder::new(&self.base_path).standard_filters(true).build() {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && !self.is_internal_path(path) {
                    if let Err(e) = self.record_change(path.to_path_buf()).await {
                        eprintln!("⚠️ Failed to sync {}: {}", path.display(), e);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn should_index(&self, path: &Path) -> bool {
        if self.is_internal_path(path) {
            return false;
        }

        // For single file checks (daemon), we do a simple .gitignore check
        // starting from the file's directory up to the base path.
        let mut builder = ignore::WalkBuilder::new(path);
        builder.standard_filters(true);
        builder.hidden(false);
        
        // If the path itself is returned by the walker, it's not ignored
        builder.build().next().is_some()
    }

    fn is_internal_path(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        path_str.contains("/.git/") || 
        path_str.contains("/.stasher/") || 
        path_str.contains("/target/") ||
        path_str.contains("/.fastembed_cache/")
    }

    pub async fn get_stats(&self) -> Result<ProjectStats> {
        let total_snapshots: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM snapshots")
            .fetch_one(&self.db.sqlite)
            .await?;
            
        let total_sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&self.db.sqlite)
            .await?;

        let objects_size = self.calculate_dir_size(&self.objects_path)?;
        let db_size = self.calculate_dir_size(&self.base_path.join(".stasher"))?;
        
        // Count files indexed in LanceDB
        let table_names: Vec<String> = self.db.lancedb.table_names().execute().await?;
        let indexed_count = if table_names.contains(&"snapshots_v1".to_string()) {
            let table = self.db.lancedb.open_table("snapshots_v1").execute().await?;
            table.count_rows(None).await? as u64
        } else {
            0
        };

        Ok(ProjectStats {
            total_snapshots,
            total_sessions,
            objects_size,
            total_size: db_size,
            indexed_count,
        })
    }

    fn calculate_dir_size(&self, path: &Path) -> Result<u64> {
        let mut size = 0;
        if path.exists() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let file_type = entry.file_type()?;
                if file_type.is_file() {
                    size += entry.metadata()?.len();
                } else if file_type.is_dir() {
                    size += self.calculate_dir_size(&entry.path())?;
                }
            }
        }
        Ok(size)
    }

    pub async fn get_snapshot_diff(&self, snapshot_id: &str) -> Result<String> {
        let diff: String = sqlx::query_scalar("SELECT diff_patch FROM snapshots WHERE id = ? OR id LIKE ?")
            .bind(snapshot_id)
            .bind(format!("{}%", snapshot_id))
            .fetch_one(&self.db.sqlite)
            .await
            .context(format!("Snapshot {} not found", snapshot_id))?;
            
        Ok(diff)
    }

    fn to_stasher_relative(&self, path_str: &str) -> String {
        let absolute = if PathBuf::from(path_str).is_absolute() {
            PathBuf::from(path_str)
        } else {
            std::env::current_dir().unwrap_or(self.base_path.clone()).join(path_str)
        };
        
        absolute.strip_prefix(&self.base_path)
            .unwrap_or(&absolute)
            .to_string_lossy()
            .to_string()
    }

    pub async fn restore_file(&self, file_path: &str, snapshot_id: Option<String>) -> Result<()> {
        let rel_path = self.to_stasher_relative(file_path);

        let (hash, actual_path): (String, String) = if let Some(id) = snapshot_id {
            sqlx::query_as("SELECT content_hash, file_path FROM snapshots WHERE id = ?")
                .bind(&id)
                .fetch_optional(&self.db.sqlite)
                .await?
                .context(format!("Snapshot ID {} not found", id))?
        } else {
            // Restore to the latest known state for this file
            sqlx::query_as("SELECT content_hash, file_path FROM snapshots WHERE file_path = ? OR file_path LIKE ? ORDER BY timestamp DESC LIMIT 1")
                .bind(&rel_path)
                .bind(format!("%/{}", file_path))
                .fetch_optional(&self.db.sqlite)
                .await?
                .context(format!("No history found for file: {}", file_path))?
        };

        let object_path = self.objects_path.join(&hash);
        let content = fs::read(object_path)
            .context("Failed to read historical object from CAS")?;

        let target_path = if PathBuf::from(&actual_path).is_absolute() {
            PathBuf::from(&actual_path)
        } else {
            self.base_path.join(&actual_path)
        };
        fs::write(target_path, content)
            .context("Failed to write restored content to disk")?;

        Ok(())
    }

    pub async fn list_all_snapshots(&self, limit: i64) -> Result<Vec<SnapshotSummary>> {
        let snapshots = sqlx::query_as::<_, SnapshotSummary>(
            "SELECT id, timestamp, lines_added, lines_removed, file_path, content_hash, diff_patch FROM snapshots 
             ORDER BY timestamp DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.db.sqlite)
        .await?;

        Ok(snapshots)
    }

    /// Auto-detect tags by analyzing diff content of all snapshots in a session.
    pub async fn auto_tag_session(&self, session_id: &str) -> Result<Vec<String>> {
        let diffs: Vec<(String,)> = sqlx::query_as(
            "SELECT diff_patch FROM snapshots WHERE session_id = ?"
        )
        .bind(session_id)
        .fetch_all(&self.db.sqlite)
        .await?;

        let combined: String = diffs.into_iter().map(|(d,)| d).collect::<Vec<_>>().join("\n");
        let tags = Self::detect_tags(&combined);

        if !tags.is_empty() {
            let tags_str = tags.join(",");
            // Merge with existing tags
            let existing: Option<(String,)> = sqlx::query_as(
                "SELECT tags FROM sessions WHERE id = ?"
            )
            .bind(session_id)
            .fetch_optional(&self.db.sqlite)
            .await?;

            let merged = if let Some((existing_tags,)) = existing {
                let mut all: Vec<String> = existing_tags
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                for t in &tags {
                    if !all.contains(t) {
                        all.push(t.clone());
                    }
                }
                all.join(",")
            } else {
                tags_str
            };

            sqlx::query("UPDATE sessions SET tags = ? WHERE id = ?")
                .bind(&merged)
                .bind(session_id)
                .execute(&self.db.sqlite)
                .await?;
        }

        Ok(tags)
    }

    /// Detect context tags from diff content using keyword patterns.
    fn detect_tags(diff_content: &str) -> Vec<String> {
        let lower = diff_content.to_lowercase();
        let mut tags = Vec::new();

        let patterns: &[(&str, &str)] = &[
            ("bugfix", r"(?i)\b(fix|bug|patch|hotfix|resolve|issue)\b"),
            ("feature", r"(?i)\b(feat|feature|add|implement|new)\b"),
            ("refactor", r"(?i)\b(refactor|restructure|reorganize|cleanup|clean up)\b"),
            ("test", r"(?i)\b(test|spec|assert|expect|mock|describe|it\()\b"),
            ("docs", r"(?i)\b(doc|readme|comment|documentation|changelog)\b"),
            ("perf", r"(?i)\b(perf|performance|optimize|cache|speed)\b"),
            ("style", r"(?i)\b(style|format|lint|prettier|eslint)\b"),
        ];

        for (tag, pattern) in patterns {
            if let Ok(re) = Regex::new(pattern) {
                // Only look at changed lines (lines starting with + or -)
                let changed_lines: String = lower
                    .lines()
                    .filter(|l| l.starts_with('+') || l.starts_with('-'))
                    .collect::<Vec<_>>()
                    .join("\n");
                if re.is_match(&changed_lines) {
                    tags.push(tag.to_string());
                }
            }
        }

        tags
    }

    /// Manually add a tag to a session.
    pub async fn add_tag_to_session(&self, session_id: &str, tag: &str) -> Result<()> {
        let existing: (String,) = sqlx::query_as(
            "SELECT COALESCE(tags, '') FROM sessions WHERE id = ?"
        )
        .bind(session_id)
        .fetch_one(&self.db.sqlite)
        .await
        .context(format!("Session {} not found", session_id))?;

        let mut tags: Vec<String> = existing.0
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let tag_str = tag.to_string();
        if !tags.contains(&tag_str) {
            tags.push(tag_str);
        }

        sqlx::query("UPDATE sessions SET tags = ? WHERE id = ?")
            .bind(tags.join(","))
            .bind(session_id)
            .execute(&self.db.sqlite)
            .await?;

        Ok(())
    }

    /// Get the session ID for a given snapshot.
    pub async fn get_session_id_for_snapshot(&self, snapshot_id: &str) -> Result<String> {
        let session_id: String = sqlx::query_scalar(
            "SELECT session_id FROM snapshots WHERE id = ? OR id LIKE ?"
        )
        .bind(snapshot_id)
        .bind(format!("{}%", snapshot_id))
        .fetch_one(&self.db.sqlite)
        .await
        .context(format!("Snapshot {} not found", snapshot_id))?;

        Ok(session_id)
    }

    /// Finalize the current session: set end_time and auto-tag.
    pub async fn finalize_current_session(&self) -> Result<()> {
        self.finalize_session(&self.current_session_id.to_string()).await
    }

    /// Finalize a session: set end_time and auto-tag.
    pub async fn finalize_session(&self, session_id: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        sqlx::query("UPDATE sessions SET end_time = ? WHERE id = ? AND end_time IS NULL")
            .bind(now)
            .bind(session_id)
            .execute(&self.db.sqlite)
            .await?;

        self.auto_tag_session(session_id).await?;
        Ok(())
    }

    /// List all sessions with tags, file count, and duration.
    pub async fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        let sessions = sqlx::query_as::<_, SessionRow>(
            "SELECT s.id, s.start_time, s.end_time, COALESCE(s.tags, '') as tags,
                    COUNT(snap.id) as file_count
             FROM sessions s
             LEFT JOIN snapshots snap ON snap.session_id = s.id
             GROUP BY s.id
             ORDER BY s.start_time DESC"
        )
        .fetch_all(&self.db.sqlite)
        .await?;

        Ok(sessions.into_iter().map(|s| {
            let duration_ms = s.end_time.unwrap_or_else(|| Utc::now().timestamp_millis()) - s.start_time;
            SessionSummary {
                id: s.id,
                start_time: s.start_time,
                end_time: s.end_time,
                tags: s.tags.split(',').filter(|t| !t.is_empty()).map(|t| t.to_string()).collect(),
                file_count: s.file_count,
                duration_secs: (duration_ms / 1000) as u64,
            }
        }).collect())
    }

    /// Get all session IDs that have a particular tag.
    #[allow(dead_code)]
    pub async fn get_session_ids_with_tag(&self, tag: &str) -> Result<Vec<String>> {
        let pattern = format!("%{}%", tag);
        let ids: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM sessions WHERE tags LIKE ?"
        )
        .bind(&pattern)
        .fetch_all(&self.db.sqlite)
        .await?;

        Ok(ids.into_iter().map(|(id,)| id).collect())
    }

    /// Get all snapshot IDs belonging to sessions with a given tag.
    pub async fn get_snapshot_ids_for_tag(&self, tag: &str) -> Result<Vec<String>> {
        let pattern = format!("%{}%", tag);
        let ids: Vec<(String,)> = sqlx::query_as(
            "SELECT snap.id FROM snapshots snap
             JOIN sessions s ON snap.session_id = s.id
             WHERE s.tags LIKE ?"
        )
        .bind(&pattern)
        .fetch_all(&self.db.sqlite)
        .await?;

        Ok(ids.into_iter().map(|(id,)| id).collect())
    }

    pub async fn list_snapshots(&self, file_path: &str) -> Result<Vec<SnapshotSummary>> {
        let rel_path = self.to_stasher_relative(file_path);
        let mut all_results = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();
        let mut queue = vec![rel_path];

        while let Some(current_search_path) = queue.pop() {
            if seen_paths.contains(&current_search_path) { continue; }
            seen_paths.insert(current_search_path.clone());

            let mut snapshots = sqlx::query_as::<_, SnapshotSummary>(
                "SELECT id, timestamp, lines_added, lines_removed, file_path, content_hash, diff_patch FROM snapshots 
                 WHERE file_path = ? OR file_path LIKE ? 
                 ORDER BY timestamp DESC"
            )
            .bind(&current_search_path)
            .bind(format!("%/{}", current_search_path))
            .fetch_all(&self.db.sqlite)
            .await?;

            if let Some(oldest) = snapshots.last() {
                // Look for predecessors: same content hash used at an earlier time in a different path
                let origins: Vec<String> = sqlx::query_scalar(
                    "SELECT DISTINCT file_path FROM snapshots 
                     WHERE content_hash = ? AND timestamp <= ? AND file_path != ?"
                )
                .bind(&oldest.content_hash)
                .bind(oldest.timestamp)
                .bind(&current_search_path)
                .fetch_all(&self.db.sqlite)
                .await?;
                
                for origin in origins {
                    if !seen_paths.contains(&origin) {
                        queue.push(origin);
                    }
                }
            }
            all_results.append(&mut snapshots);
        }
        
        // Sort by timestamp descending (newest first), then remove duplicates (if any from overlapping history)
        all_results.sort_by_key(|s| std::cmp::Reverse(s.timestamp));
        all_results.dedup_by(|a, b| a.id == b.id);
        
        Ok(all_results)
    }
}

#[derive(sqlx::FromRow, serde::Serialize)]
pub struct SnapshotSummary {
    pub id: String,
    pub timestamp: i64,
    pub lines_added: i32,
    pub lines_removed: i32,
    pub file_path: String,
    pub content_hash: String,
    pub diff_patch: String,
}

#[derive(serde::Serialize)]
pub struct ProjectStats {
    pub total_snapshots: i64,
    pub total_sessions: i64,
    pub objects_size: u64,
    pub total_size: u64,
    pub indexed_count: u64,
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    pub id: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub tags: String,
    pub file_count: i32,
}

#[derive(serde::Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub tags: Vec<String>,
    pub file_count: i32,
    pub duration_secs: u64,
}
