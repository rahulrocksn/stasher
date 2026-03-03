use axum::{
    routing::{get, post},
    Json, Router, extract::{Query, State},
    http::StatusCode,
};
use tower_http::cors::{Any, CorsLayer};
use std::net::SocketAddr;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
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

pub async fn start_server() -> Result<()> {
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
) -> Result<Json<Vec<ProjectInfo>>, (StatusCode, String)> {
    let projects = state.hub.list_projects().await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(projects))
}

#[derive(Deserialize)]
struct PathParams {
    project_path: String,
    file_path: String,
}

async fn list_snapshots(
    State(_state): State<AppState>,
    Query(params): Query<PathParams>,
) -> Result<Json<Vec<SnapshotSummary>>, (StatusCode, String)> {
    let base_path = PathBuf::from(&params.project_path);
    if !base_path.exists() {
        return Err((StatusCode::NOT_FOUND, "Project path not found".into()));
    }

    let db = Database::init(&base_path).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let history = HistoryManager::new(Arc::new(db), base_path).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let snapshots = history.list_snapshots(&params.file_path).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(snapshots))
}

async fn search(
    State(state): State<AppState>,
    Json(params): Json<SearchParams>,
) -> Result<Json<Vec<SearchResultUI>>, (StatusCode, String)> {
    let mut all_results = Vec::new();

    if params.global.unwrap_or(true) {
        let projects = state.hub.list_projects().await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
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
