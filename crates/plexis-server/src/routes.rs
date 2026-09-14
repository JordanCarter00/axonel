//! REST API routes and handlers for Plexis Server.

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use plexis_core::ids::{ApprovalId, TaskId, WorkflowId};
use plexis_core::state::TaskState;
use plexis_core::{ApprovalRecord, Command, Event, Workflow};
use plexis_storage::traits::{
    AgentStore, ApprovalStore, CommandStore, EventStore, TaskStore, WorkflowStore,
};

use crate::state::AppState;

/// Creates the complete Axum router configured with all API endpoints.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/system/status", get(system_status))
        .route(
            "/api/v1/workflows",
            get(list_workflows).post(create_workflow),
        )
        .route("/api/v1/workflows/{id}", get(get_workflow))
        .route("/api/v1/tasks/{id}", get(get_task))
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/commands", post(enqueue_command))
        .route("/api/v1/events", get(list_recent_events))
        .route("/api/v1/approvals", get(list_approvals))
        .route("/api/v1/approvals/{id}", get(get_approval))
        .route("/api/v1/approvals/{id}/approve", post(approve_gate))
        .route("/api/v1/approvals/{id}/reject", post(reject_gate))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
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

/// Standardized structured JSON error envelope for API consumers.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub status: u16,
}

impl ApiError {
    pub fn new(status: StatusCode, error: impl Into<String>) -> (StatusCode, Json<ApiError>) {
        (
            status,
            Json(ApiError {
                error: error.into(),
                status: status.as_u16(),
            }),
        )
    }
}

#[derive(Deserialize)]
pub struct CreateWorkflowRequest {
    pub title: String,
    pub objective: String,
}

async fn list_workflows(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let workflows = state
        .store
        .list_workflows()
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(workflows))
}

async fn create_workflow(
    State(state): State<AppState>,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<(StatusCode, Json<Workflow>), (StatusCode, Json<ApiError>)> {
    if req.title.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Workflow title cannot be empty",
        ));
    }
    if req.objective.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Workflow objective cannot be empty",
        ));
    }

    let wf = Workflow::new(req.title, req.objective);
    state
        .store
        .create_workflow(&wf)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((StatusCode::CREATED, Json(wf)))
}

async fn get_workflow(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Workflow>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let wf = state
        .store
        .get_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Workflow {} not found", id))
        })?;
    Ok(Json(wf))
}

async fn get_task(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<plexis_core::Task>, (StatusCode, Json<ApiError>)> {
    let id: TaskId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid task id: {}", id_str),
        )
    })?;
    let task = state
        .store
        .get_task(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("Task {} not found", id)))?;
    Ok(Json(task))
}

async fn list_agents(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let agents = state
        .store
        .list_agents()
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(agents))
}

async fn enqueue_command(
    State(state): State<AppState>,
    Json(cmd): Json<Command>,
) -> Result<(StatusCode, Json<Command>), (StatusCode, Json<ApiError>)> {
    if cmd.idempotency_key.trim().is_empty() {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "Command idempotency_key cannot be empty",
        ));
    }

    match state.store.enqueue_command(&cmd).await {
        Ok(_) => Ok((StatusCode::ACCEPTED, Json(cmd))),
        Err(plexis_storage::StorageError::IdempotencyConflict(_)) => Err(ApiError::new(
            StatusCode::CONFLICT,
            "Command with this idempotency key already exists",
        )),
        Err(e) => Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            e.to_string(),
        )),
    }
}

async fn list_recent_events(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let events = state
        .store
        .list_recent_events(50)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(events))
}

#[derive(Debug, Deserialize, Default)]
pub struct DecisionPayload {
    pub decider: Option<String>,
    pub note: Option<String>,
}

async fn list_approvals(
    State(state): State<AppState>,
) -> Result<Json<Vec<ApprovalRecord>>, (StatusCode, Json<ApiError>)> {
    let approvals = state
        .store
        .list_pending_approvals()
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(approvals))
}

async fn get_approval(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<ApprovalRecord>, (StatusCode, Json<ApiError>)> {
    let id: ApprovalId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid approval id: {}", id_str),
        )
    })?;
    let approval = state
        .store
        .get_approval(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Approval {} not found", id))
        })?;
    Ok(Json(approval))
}

async fn approve_gate(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Json(payload): Json<DecisionPayload>,
) -> Result<Json<ApprovalRecord>, (StatusCode, Json<ApiError>)> {
    let id: ApprovalId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid approval id: {}", id_str),
        )
    })?;
    let mut approval = state
        .store
        .get_approval(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Approval {} not found", id))
        })?;

    let decider = payload.decider.as_deref().unwrap_or("operator");
    approval
        .approve(payload.note)
        .map_err(|e| ApiError::new(StatusCode::CONFLICT, e.to_string()))?;

    state
        .store
        .update_approval(&approval)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // If there is an associated task waiting in NeedsHuman, unblock it to Ready
    if let Ok(Some(mut task)) = state.store.get_task(&approval.task_id).await {
        if task.state == TaskState::NeedsHuman {
            let _ = task.transition_to(TaskState::Ready);
            let _ = state.store.update_task(&task).await;
        }
    }

    let evt = Event::new(
        "approval",
        approval.id.to_string(),
        "approval.gate_decided",
        serde_json::json!({
            "decision": "approved",
            "decider": decider,
        }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(approval))
}

async fn reject_gate(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Json(payload): Json<DecisionPayload>,
) -> Result<Json<ApprovalRecord>, (StatusCode, Json<ApiError>)> {
    let id: ApprovalId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid approval id: {}", id_str),
        )
    })?;
    let mut approval = state
        .store
        .get_approval(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Approval {} not found", id))
        })?;

    let decider = payload.decider.as_deref().unwrap_or("operator");
    let reason = payload
        .note
        .unwrap_or_else(|| "Rejected by operator".to_string());
    approval
        .reject(Some(reason))
        .map_err(|e| ApiError::new(StatusCode::CONFLICT, e.to_string()))?;

    state
        .store
        .update_approval(&approval)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // If there is an associated task waiting in NeedsHuman, move to Failed
    if let Ok(Some(mut task)) = state.store.get_task(&approval.task_id).await {
        if task.state == TaskState::NeedsHuman {
            let _ = task.transition_to(TaskState::Failed);
            let _ = state.store.update_task(&task).await;
        }
    }

    let evt = Event::new(
        "approval",
        approval.id.to_string(),
        "approval.gate_decided",
        serde_json::json!({
            "decision": "rejected",
            "decider": decider,
        }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(approval))
}
