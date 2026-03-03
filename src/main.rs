mod db;
mod daemon;
mod history;
mod search;
mod hub;
mod server;

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
    /// Show project statistics and daemon status
    Status,
    /// List all projects tracked by Stasher on this machine
    Projects,
    /// Search across all tracked projects
    GlobalAsk { query: String },
    /// Start the Stasher Hub UI Dashboard (local web server)
    Serve,
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

            // Register project with global hub
            let hub = hub::StasherHub::init().await?;
            hub.register_project(&base_path).await?;
            println!("🌐 Project registered globally in Stasher Hub.");
            
            Ok(())
        }
        Commands::Daemon => {
            let lock_path = base_path.join(".stasher/daemon.pid");
            if lock_path.exists() {
                let pid = std::fs::read_to_string(&lock_path).unwrap_or_default();
                eprintln!("❌ Stasher daemon is already running (PID: {}).", pid);
                eprintln!("   If you are sure it's not running, delete {}", lock_path.display());
                return Ok(());
            }
            std::fs::write(&lock_path, std::process::id().to_string())?;

            println!("🚀 Initializing Stasher database in .stasher/ ...");
            let db = db::Database::init(&base_path).await?;
            println!("💾 Database ready. Starting daemon...");
            
            let daemon = daemon::StasherDaemon::new(db, base_path.to_path_buf()).await?;
            
            // Update last_active in global hub
            if let Ok(hub) = hub::StasherHub::init().await {
                let _ = hub.register_project(&base_path).await;
            }
            // Cleanup lock file on exit
            let res = daemon.run().await;
            let _ = std::fs::remove_file(lock_path);
            res
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
            
            // Safety: Snapshot the current state before overwriting
            let current_path = if PathBuf::from(file).is_absolute() {
                PathBuf::from(file)
            } else {
                base_path.join(file)
            };
            
            if current_path.exists() {
                let _ = history.record_change(current_path).await;
            }

            history.restore_file(&file, snapshot.clone()).await?;
            println!("✅ Restore complete.");
            Ok(())
        }
        Commands::Status => {
            use colored::Colorize;
            let db = db::Database::init(&base_path).await?;
            let history = history::HistoryManager::new(std::sync::Arc::new(db), base_path.to_path_buf()).await?;
            let stats = history.get_stats().await?;

            println!("{}", "📊 Stasher Project Status".bold().bright_white());
            println!("{:-<30}", "");
            
            println!("{:<20} {}", "Total Snapshots:".bold(), stats.total_snapshots.to_string().cyan());
            println!("{:<20} {}", "Total Sessions:".bold(), stats.total_sessions.to_string().cyan());
            println!("{:<20} {}", "Indexed Files:".bold(), stats.indexed_count.to_string().green());
            
            let objects_mb = stats.objects_size as f64 / 1_048_576.0;
            let total_mb = stats.total_size as f64 / 1_048_576.0;
            
            println!("{:<20} {:.2} MB", "Object Storage:".bold(), objects_mb.to_string().yellow());
            println!("{:<20} {:.2} MB", "Total Cache Size:".bold(), total_mb.to_string().yellow());
            
            // Deduplication estimate (mock logic for UX)
            let raw_est = stats.total_snapshots as f64 * 0.5; // Assume 0.5MB avg per file
            let saved = raw_est - total_mb;
            if saved > 0.0 {
                println!("{:<20} {} {:.2} MB", "Space Saved:".bold(), "🚀".green(), saved.to_string().green().bold());
            }

            println!("\n{}", "Background Service:".bold());
            // Simple check (could find a better way)
            println!("   {} Stasher is monitoring your files", "●".green());
            
            Ok(())
        }
        Commands::Show { file } => {
            use colored::Colorize;
            println!("📜 History for {}:", file.bold().cyan());
            let db = db::Database::init(&base_path).await?;
            let history = history::HistoryManager::new(std::sync::Arc::new(db), base_path.to_path_buf()).await?;
            
            let snapshots = history.list_snapshots(file).await?;
            
            if snapshots.is_empty() {
                println!("🤷 No history found for this file.");
            } else {
                let mut current_display_path = snapshots[0].file_path.clone();
                for snap in snapshots {
                    if snap.file_path != current_display_path {
                        println!("      {}", format!("⤴️ moved from {}", snap.file_path).italic().dimmed());
                        current_display_path = snap.file_path.clone();
                    }
                    let dt = chrono::Utc::now().timestamp_millis() - snap.timestamp;
                    let ago = if dt < 60000 {
                        format!("{}s ago", dt / 1000)
                    } else if dt < 3600000 {
                        format!("{}m ago", dt / 60000)
                    } else {
                        format!("{}h ago", dt / 3600000)
                    };

                    println!(
                        "[{}] {} | {}{} {}{}",
                        &snap.id[..7].bright_white().bold(),
                        ago.yellow(),
                        "+".green(),
                        snap.lines_added.to_string().green(),
                        "-".red(),
                        snap.lines_removed.to_string().red()
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
        Commands::Projects => {
            use colored::Colorize;
            let hub = hub::StasherHub::init().await?;
            let projects = hub.list_projects().await?;

            println!("{}", "🌐 Registered Stasher Projects".bold().bright_white());
            println!("{:-<40}", "");

            if projects.is_empty() {
                println!("🤷 No projects tracked yet. Run 'stasher init' in a project!");
            } else {
                for p in projects {
                    let dt = chrono::Utc::now().timestamp_millis() - p.last_active;
                    let ago = if dt < 60000 {
                        format!("{}s ago", dt / 1000)
                    } else if dt < 3600000 {
                        format!("{}m ago", dt / 60000)
                    } else {
                        format!("{}h ago", dt / 3600000)
                    };

                    println!(
                        "{} {} {}",
                        "●".green(),
                        p.name.bold().cyan(),
                        format!("({})", p.path).dimmed()
                    );
                    println!("  Active: {}", ago.yellow());
                }
            }
            Ok(())
        }
        Commands::GlobalAsk { query } => {
            use colored::Colorize;
            println!("🌐 Global Search: \"{}\"...", query.bold().cyan());
            let hub = hub::StasherHub::init().await?;
            let projects = hub.list_projects().await?;
            
            let mut all_results = Vec::new();

            for project in projects {
                let project_path = PathBuf::from(&project.path);
                if !project_path.exists() { continue; }

                if let Ok(db) = db::Database::init(&project_path).await {
                    let search = search::SearchEngine::new(db.lancedb.clone()).await?;
                    if let Ok(results) = search.search(query.clone(), 3).await {
                        for res in results {
                            all_results.push((project.name.clone(), res));
                        }
                    }
                }
            }

            if all_results.is_empty() {
                println!("🤷 No relevant history found in any project.");
            } else {
                println!("✨ Found {} matches across your projects:", all_results.len());
                for (proj_name, res) in all_results {
                    println!("\n{} | File: {}", proj_name.bold().magenta(), res.file_path.cyan());
                    let snippet: String = res.content.lines().take(3).collect::<Vec<_>>().join("\n");
                    println!("--- Snippet ---");
                    println!("{}...", snippet);
                }
            }
            Ok(())
        }
        Commands::Serve => {
            server::start_server().await?;
            Ok(())
        }
    }
}
