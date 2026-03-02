mod db;
mod daemon;
mod history;
mod search;

use std::path::{Path, PathBuf};
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "stasher")]
#[command(about = "Local-first development history tracker", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Stasher history in the current directory
    Init,
    /// Start the background daemon
    Daemon,
    /// Search history using natural language
    Ask { query: String },
    /// Show history for a file
    Show { file: String },
    /// Restore a file to a previous version
    Restore { 
        file: String, 
        #[arg(short, long)]
        snapshot: Option<String> 
    },
    /// Clean up old snapshots and unused objects
    Prune {
        #[arg(short, long, default_value_t = 30)]
        days: u32,
    },
    /// Show the differences recorded in a specific snapshot
    Diff { snapshot: String },
}

fn find_stasher_root(start_path: &Path) -> Option<PathBuf> {
    let mut current = start_path.to_path_buf();
    loop {
        if current.join(".stasher").exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let current_dir = std::env::current_dir()?;
    
    // For Init, we use current directory. For others, we try to find the root.
    let base_path = if matches!(cli.command, Commands::Init) {
        current_dir.clone()
    } else {
        find_stasher_root(&current_dir).unwrap_or(current_dir.clone())
    };

    match &cli.command {
        Commands::Init => {
            println!("🚀 Initializing Stasher repository in {}...", base_path.display());
            let db = db::Database::init(&base_path).await?;
            let history = history::HistoryManager::new(std::sync::Arc::new(db), base_path.to_path_buf()).await?;
            
            // Perform initial sync of all files
            history.sync_all().await?;

            // Check if .gitignore exists and add .stasher if missing
            let gitignore_path = base_path.join(".gitignore");
            if gitignore_path.exists() {
                let current_content = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
                if !current_content.contains(".stasher/") {
                    use std::fs::OpenOptions;
                    use std::io::Write;
                    let mut file = OpenOptions::new()
                        .append(true)
                        .open(&gitignore_path)?;
                    writeln!(file, "\n# Stasher local history\n.stasher/")?;
                    println!("📝 Added .stasher/ to .gitignore");
                }
            }

            println!("✨ Stasher initialized successfully!");
            Ok(())
        }
        Commands::Daemon => {
            println!("🚀 Initializing Stasher database in .stasher/ ...");
            let db = db::Database::init(&base_path).await?;
            println!("💾 Database ready. Starting daemon...");
            
            let daemon = daemon::StasherDaemon::new(db, base_path.to_path_buf()).await?;
            daemon.run().await
        }
        Commands::Ask { query } => {
            println!("🔍 Searching for: \"{}\"...", query);
            let db = db::Database::init(&base_path).await?;
            let search = search::SearchEngine::new(db.lancedb.clone()).await?;
            
            let results = search.search(query.clone(), 5).await?;
            
            if results.is_empty() {
                println!("🤷 No relevant history found.");
            } else {
                println!("✨ Found {} relevant snapshots:", results.len());
                for (i, res) in results.iter().enumerate() {
                    println!("\n[{}] File: {}", i + 1, res.file_path);
                    println!("--- Snippet ---");
                    let snippet: String = res.content.lines().take(5).collect::<Vec<_>>().join("\n");
                    println!("{}...", snippet);
                }
            }
            Ok(())
        }
        Commands::Restore { file, snapshot } => {
            println!("⏪ Restoring {}...", file);
            let db = db::Database::init(&base_path).await?;
            let history = history::HistoryManager::new(std::sync::Arc::new(db), base_path.to_path_buf()).await?;
            
            history.restore_file(&file, snapshot.clone()).await?;
            println!("✅ Restore complete.");
            Ok(())
        }
        Commands::Show { file } => {
            println!("📜 History for {}:", file);
            let db = db::Database::init(&base_path).await?;
            let history = history::HistoryManager::new(std::sync::Arc::new(db), base_path.to_path_buf()).await?;
            
            let snapshots = history.list_snapshots(file).await?;
            
            if snapshots.is_empty() {
                println!("🤷 No history found for this file.");
            } else {
                for snap in snapshots {
                    let dt = chrono::Utc::now().timestamp_millis() - snap.timestamp;
                    let ago = if dt < 60000 {
                        format!("{}s ago", dt / 1000)
                    } else if dt < 3600000 {
                        format!("{}m ago", dt / 60000)
                    } else {
                        format!("{}h ago", dt / 3600000)
                    };

                    println!(
                        "[{}] {} | +{}, -{} lines",
                        &snap.id[..7],
                        ago,
                        snap.lines_added,
                        snap.lines_removed
                    );
                }
            }
            Ok(())
        }
        Commands::Prune { days } => {
            println!("🧹 Pruning snapshots older than {} days...", days);
            let db = db::Database::init(&base_path).await?;
            let history = history::HistoryManager::new(std::sync::Arc::new(db), base_path.to_path_buf()).await?;
            
            let (deleted_snaps, deleted_objs) = history.prune_history(*days).await?;
            println!("✅ Cleanup complete:");
            println!("   - {} snapshots removed from database", deleted_snaps);
            println!("   - {} unused objects deleted from disk", deleted_objs);
            Ok(())
        }
        Commands::Diff { snapshot } => {
            use colored::Colorize;
            let db = db::Database::init(&base_path).await?;
            let history = history::HistoryManager::new(std::sync::Arc::new(db), base_path.to_path_buf()).await?;
            
            let diff = history.get_snapshot_diff(snapshot).await?;
            println!("📑 Diff for snapshot {}:", snapshot.cyan());
            
            for line in diff.lines() {
                if line.starts_with('+') && !line.starts_with("+++") {
                    println!("{}", line.green());
                } else if line.starts_with('-') && !line.starts_with("---") {
                    println!("{}", line.red());
                } else if line.starts_with("@@") {
                    println!("{}", line.blue());
                } else {
                    println!("{}", line);
                }
            }
            Ok(())
        }
    }
}
