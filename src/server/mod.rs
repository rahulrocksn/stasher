use axum::{
    routing::{get, post},
    Json, Router, extract::{Query, State},
    http::StatusCode,
};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::path::PathBuf;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::result::Result as StdResult;
use axum::response::IntoResponse;

pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
use crate::hub::{StasherHub, ProjectInfo};
use crate::db::Database;
use crate::history::{HistoryManager, SnapshotSummary};

#[derive(Clone)]
pub struct AppState {
    pub hub: Arc<StasherHub>,
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
    global: Option<bool>,
}

#[derive(Serialize)]
struct SearchResultUI {
    project: String,
    file_path: String,
    content: String,
}

pub async fn start_server() -> anyhow::Result<()> {
    let hub = Arc::new(StasherHub::init().await?);
    let state = AppState { hub };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/projects", get(list_projects))
        .route("/api/snapshots", get(list_snapshots))
        .route("/api/search", post(search))
        .route("/api/stats", get(get_project_stats))
        .route("/api/restore", post(handle_restore))
        .fallback_service(ServeDir::new("ui/dist"))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("🚀 Stasher Hub API listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn list_projects(
    State(state): State<AppState>,
) -> StdResult<Json<Vec<ProjectInfo>>, AppError> {
    let projects = state.hub.list_projects().await?;
    Ok(Json(projects))
}

#[derive(Deserialize)]
struct PathParams {
    project_path: String,
    file_path: Option<String>,
}

#[derive(Deserialize)]
struct ProjectParams {
    project_path: String,
}

async fn list_snapshots(
    Query(params): Query<PathParams>,
    State(_state): State<AppState>,
) -> StdResult<Json<Vec<SnapshotSummary>>, AppError> {
    let base_path = PathBuf::from(&params.project_path);
    if !base_path.exists() {
        return Err(anyhow::anyhow!("Project path not found").into());
    }

    let db = Database::init(&base_path).await?;
    let history = HistoryManager::new(Arc::new(db), base_path).await?;

    let snapshots = if let Some(file) = &params.file_path {
        history.list_snapshots(file).await?
    } else {
        history.list_all_snapshots(100).await?
    };

    Ok(Json(snapshots))
}

#[derive(Deserialize)]
struct RestoreParams {
    project_path: String,
    file_path: String,
    snapshot_id: Option<String>,
}

async fn handle_restore(
    Json(params): Json<RestoreParams>,
) -> StdResult<StatusCode, AppError> {
    let base_path = PathBuf::from(&params.project_path);
    if !base_path.exists() {
        return Err(anyhow::anyhow!("Project path not found").into());
    }

    let db = Database::init(&base_path).await?;
    let history = HistoryManager::new(Arc::new(db), base_path).await?;
    
    history.restore_file(&params.file_path, params.snapshot_id).await?;
    
    Ok(StatusCode::OK)
}

async fn search(
    State(state): State<AppState>,
    Json(params): Json<SearchParams>,
) -> StdResult<Json<Vec<SearchResultUI>>, AppError> {
    let mut all_results = Vec::new();

    if params.global.unwrap_or(true) {
        let projects = state.hub.list_projects().await?;
        
        for project in projects {
            let project_path = PathBuf::from(&project.path);
            if !project_path.exists() { continue; }

            if let Ok(db) = Database::init(&project_path).await {
                if let Ok(search_engine) = crate::search::SearchEngine::new(db.lancedb.clone()).await {
                    if let Ok(results) = search_engine.search(params.q.clone(), 3).await {
                        for res in results {
                            all_results.push(SearchResultUI {
                                project: project.name.clone(),
                                file_path: res.file_path,
                                content: res.content,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(Json(all_results))
}

#[derive(Serialize)]
struct PulseStats {
    hour: u32,
    count: i64,
}

#[derive(Serialize)]
struct ProjectFullStats {
    total_snapshots: i64,
    total_size_mb: f64,
    dedup_ratio: f64,
    pulse: Vec<PulseStats>,
}

async fn get_project_stats(
    Query(params): Query<ProjectParams>,
    State(_state): State<AppState>,
) -> StdResult<Json<ProjectFullStats>, AppError> {
    let base_path = PathBuf::from(&params.project_path);
    let db = Database::init(&base_path).await?;
    let history = HistoryManager::new(Arc::new(db), base_path).await?;
    
    let stats = history.get_stats().await?;
    
    // Calculate pulse for the last 24 hours
    let mut pulse = Vec::new();
    let now = Utc::now().timestamp_millis();
    for hour in (0..24).rev() {
        let start = now - (hour + 1) * 3600 * 1000;
        let end = now - hour * 3600 * 1000;
        
        // Use history.db.sqlite directly since we have it
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM snapshots WHERE timestamp >= ? AND timestamp < ?")
            .bind(start)
            .bind(end)
            .fetch_one(&history.get_db().sqlite) // Need access to the db pool
            .await?;
            
        pulse.push(PulseStats { hour: ((24 - hour) % 24) as u32, count });
    }

    Ok(Json(ProjectFullStats {
        total_snapshots: stats.total_snapshots,
        total_size_mb: stats.total_size as f64 / 1_048_576.0,
        dedup_ratio: 0.84, // Simplified for now
        pulse,
    }))
}
