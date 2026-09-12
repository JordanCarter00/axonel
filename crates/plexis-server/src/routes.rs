//! REST API routes and handlers for Plexis Server.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use plexis_core::ids::{TaskId, WorkflowId};
use plexis_core::{Command, Workflow};
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, TaskStore, WorkflowStore,
};

use crate::state::AppState;

/// Creates the complete Axum router configured with all API endpoints.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/system/status", get(system_status))
        .route("/api/v1/workflows", get(list_workflows).post(create_workflow))
        .route("/api/v1/workflows/{id}", get(get_workflow))
        .route("/api/v1/tasks/{id}", get(get_task))
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/commands", post(enqueue_command))
        .route("/api/v1/events", get(list_recent_events))
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn health_check() -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Serialize)]
struct SystemStatusResponse {
    system: &'static str,
    database: &'static str,
    version: &'static str,
}

async fn system_status() -> impl IntoResponse {
    Json(SystemStatusResponse {
        system: "plexis-control-plane",
        database: "sqlite-authoritative",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[derive(Deserialize)]
pub struct CreateWorkflowRequest {
    pub title: String,
    pub objective: String,
}

async fn list_workflows(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let workflows = state
        .store
        .list_workflows()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(workflows))
}

async fn create_workflow(
    State(state): State<AppState>,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let wf = Workflow::new(req.title, req.objective);
    state
        .store
        .create_workflow(&wf)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok((StatusCode::CREATED, Json(wf)))
}

async fn get_workflow(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let id: WorkflowId = id_str.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let wf = state
        .store
        .get_workflow(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(wf))
}

async fn get_task(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let id: TaskId = id_str.parse().map_err(|_| StatusCode::BAD_REQUEST)?;
    let task = state
        .store
        .get_task(&id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(task))
}

async fn list_agents(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let agents = state
        .store
        .list_agents()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(agents))
}

async fn enqueue_command(
    State(state): State<AppState>,
    Json(cmd): Json<Command>,
) -> Result<impl IntoResponse, StatusCode> {
    match state.store.enqueue_command(&cmd).await {
        Ok(_) => Ok((StatusCode::ACCEPTED, Json(cmd))),
        Err(plexis_storage::StorageError::IdempotencyConflict(_)) => {
            Err(StatusCode::CONFLICT)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn list_recent_events(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let events = state
        .store
        .list_recent_events(50)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(events))
}
