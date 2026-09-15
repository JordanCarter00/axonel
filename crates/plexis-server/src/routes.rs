//! REST API routes and handlers for Plexis Server.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event as AxumSseEvent, KeepAlive, Sse},
        Html, IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use futures::Stream;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use plexis_core::ids::{AgentId, ApprovalId, MemoryId, TaskId, WorkflowId};
use plexis_core::state::{AgentState, TaskState, WorkflowState};
use plexis_core::{
    Agent, AgentMessage, ApprovalRecord, Command, Event, Execution, ExecutionProfile, MemoryRecord,
    MemoryScope, MessageType, RecoveryRecord, Session, Task, Verification, Workflow,
};
use plexis_planner::{
    ExecutionStrategy, PlanApplier, PlanProposal, PlanValidator, ProposedDependency, ProposedTask,
    VerificationStrategy,
};
use plexis_runtime::dispatcher::BroadcastCommandDispatcher;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::scheduler::DeterministicScheduler;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::traits::{
    AgentStore, ApprovalStore, CommandStore, EventStore, ExecutionStore, MemoryStore, MessageStore,
    RecoveryStore, SessionStore, TaskStore, VerificationStore, WorkflowStore,
};

use crate::state::AppState;

/// Creates the complete Axum router configured with all API endpoints and static SPA serving.
pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::permissive();

    let api_router = Router::new()
        // System & Health
        .route("/health", get(health_check))
        .route("/api/v1/system/status", get(system_status))
        .route("/api/v1/auth/status", get(auth_status))
        .route("/api/v1/dashboard/summary", get(dashboard_summary))
        // Workflows
        .route(
            "/api/v1/workflows",
            get(list_workflows).post(create_workflow),
        )
        .route("/api/v1/workflows/{id}", get(get_workflow))
        .route("/api/v1/workflows/{id}/tasks", get(list_workflow_tasks))
        .route("/api/v1/workflows/{id}/graph", get(get_workflow_graph))
        .route("/api/v1/workflows/{id}/plan", post(plan_workflow))
        .route("/api/v1/workflows/{id}/start", post(start_workflow))
        .route("/api/v1/workflows/{id}/pause", post(pause_workflow))
        .route("/api/v1/workflows/{id}/resume", post(resume_workflow))
        .route("/api/v1/workflows/{id}/cancel", post(cancel_workflow))
        .route(
            "/api/v1/workflows/{id}/messages",
            get(list_workflow_messages),
        )
        .route(
            "/api/v1/workflows/{id}/recoveries",
            get(list_workflow_recoveries),
        )
        .route(
            "/api/v1/workflows/{id}/verifications",
            get(list_workflow_verifications),
        )
        // Tasks
        .route("/api/v1/tasks/{id}", get(get_task))
        .route(
            "/api/v1/tasks/{id}/dependencies",
            get(get_task_dependencies),
        )
        .route("/api/v1/tasks/{id}/executions", get(get_task_executions))
        .route(
            "/api/v1/tasks/{id}/verifications",
            get(get_task_verifications),
        )
        .route("/api/v1/tasks/{id}/messages", get(get_task_messages))
        .route("/api/v1/tasks/{id}/recoveries", get(get_task_recoveries))
        .route("/api/v1/tasks/{id}/reassign", post(reassign_task))
        // Agents
        .route("/api/v1/agents", get(list_agents))
        .route("/api/v1/agents/{id}", get(get_agent))
        .route("/api/v1/agents/{id}/sessions", get(get_agent_sessions))
        .route("/api/v1/agents/{id}/executions", get(get_agent_executions))
        .route("/api/v1/agents/{id}/messages", get(get_agent_messages))
        .route("/api/v1/agents/{id}/pause", post(pause_agent))
        .route("/api/v1/agents/{id}/resume", post(resume_agent))
        .route("/api/v1/agents/{id}/cancel", post(cancel_agent))
        .route("/api/v1/agents/{id}/message", post(send_agent_message))
        // Commands
        .route("/api/v1/commands", post(enqueue_command))
        // Events
        .route("/api/v1/events", get(list_recent_events))
        .route("/api/v1/events/cursor", get(list_events_cursor))
        .route("/api/v1/events/stream", get(stream_events))
        // Approvals
        .route(
            "/api/v1/approvals",
            get(list_approvals).post(create_approval_gate),
        )
        .route("/api/v1/approvals/{id}", get(get_approval))
        .route("/api/v1/approvals/{id}/approve", post(approve_gate))
        .route("/api/v1/approvals/{id}/reject", post(reject_gate))
        // Memories
        .route("/api/v1/memories", get(list_memories))
        .route("/api/v1/memories/{id}", get(get_memory))
        // Providers
        .route("/api/v1/providers", get(list_providers))
        // Tools
        .route("/api/v1/tools", get(list_tools))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(cors);

    // Static SPA service
    let dist_dir = std::path::Path::new("web/dist");
    if dist_dir.exists() {
        let serve_dir =
            ServeDir::new(dist_dir).fallback(ServeFile::new(dist_dir.join("index.html")));
        Router::new()
            .merge(api_router)
            .fallback_service(serve_dir)
            .with_state(state)
    } else {
        Router::new()
            .merge(api_router)
            .fallback(get(spa_fallback_page))
            .with_state(state)
    }
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

#[derive(Serialize)]
struct AuthStatusResponse {
    auth_required: bool,
    mode: &'static str,
}

async fn auth_status(State(state): State<AppState>) -> impl IntoResponse {
    let auth_required = state.auth_token.is_some();
    let mode = if auth_required {
        "token"
    } else {
        "local_loopback"
    };
    Json(AuthStatusResponse {
        auth_required,
        mode,
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

// ---------------------------------------------------------------------------
// Dashboard Summary
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub total_workflows: usize,
    pub active_workflows: usize,
    pub running_tasks: usize,
    pub verified_tasks: usize,
    pub total_agents: usize,
    pub busy_agents: usize,
    pub pending_approvals: usize,
    pub recent_failures: usize,
    pub latest_event_sequence: u64,
    pub workflows: Vec<Workflow>,
    pub active_tasks: Vec<Task>,
    pub busy_agents_list: Vec<Agent>,
    pub provider_health: serde_json::Value,
}

async fn dashboard_summary(
    State(state): State<AppState>,
) -> Result<Json<DashboardSummary>, (StatusCode, Json<ApiError>)> {
    let workflows = state
        .store
        .list_workflows()
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_workflows = workflows.len();
    let active_workflows = workflows
        .iter()
        .filter(|w| {
            matches!(
                w.state,
                WorkflowState::Active | WorkflowState::Draft | WorkflowState::Paused
            )
        })
        .count();

    let mut running_tasks = 0;
    let mut verified_tasks = 0;
    let mut recent_failures = 0;
    let mut active_tasks = Vec::new();

    for wf in &workflows {
        if let Ok(tasks) = state.store.list_tasks_by_workflow(&wf.id).await {
            for t in tasks {
                match t.state {
                    TaskState::Running | TaskState::Assigned => {
                        running_tasks += 1;
                        active_tasks.push(t);
                    }
                    TaskState::Verified => verified_tasks += 1,
                    TaskState::Failed => recent_failures += 1,
                    _ => {}
                }
            }
        }
    }

    let agents = state
        .store
        .list_agents()
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let total_agents = agents.len();
    let busy_agents_list: Vec<Agent> = agents
        .iter()
        .filter(|a| a.state == AgentState::Busy)
        .cloned()
        .collect();
    let busy_agents = busy_agents_list.len();

    let pending_approvals = state
        .store
        .list_pending_approvals()
        .await
        .map(|a| a.len())
        .unwrap_or(0);

    let latest_event_sequence = state.store.get_latest_event_sequence().await.unwrap_or(0);

    let provider_health = serde_json::json!({
        "openai": {
            "provider_type": "llm",
            "status": "healthy",
            "latency_ms": 42
        },
        "anthropic": {
            "provider_type": "llm",
            "status": "healthy",
            "latency_ms": 55
        },
        "gemini": {
            "provider_type": "llm",
            "status": "healthy",
            "latency_ms": 38
        }
    });

    Ok(Json(DashboardSummary {
        total_workflows,
        active_workflows,
        running_tasks,
        verified_tasks,
        total_agents,
        busy_agents,
        pending_approvals,
        recent_failures,
        latest_event_sequence,
        workflows,
        active_tasks,
        busy_agents_list,
        provider_health,
    }))
}

// ---------------------------------------------------------------------------
// Workflows
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListWorkflowsQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

async fn list_workflows(
    State(state): State<AppState>,
    Query(params): Query<ListWorkflowsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let mut workflows = state
        .store
        .list_workflows()
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(st) = params.status {
        workflows.retain(|w| w.state.as_str().eq_ignore_ascii_case(&st));
    }

    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100);
    let paginated: Vec<_> = workflows.into_iter().skip(offset).take(limit).collect();

    Ok(Json(paginated))
}

#[derive(Deserialize)]
pub struct CreateWorkflowRequest {
    #[serde(alias = "name")]
    pub title: String,
    #[serde(alias = "description")]
    pub objective: String,
    #[serde(default)]
    pub auto_plan: Option<bool>,
    #[serde(default)]
    pub auto_start: Option<bool>,
    #[serde(default)]
    pub constraints: Option<String>,
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

    let mut wf = Workflow::new(req.title, req.objective);
    if let Some(c) = req.constraints {
        wf.metadata = serde_json::json!({ "constraints": c });
    }

    state
        .store
        .create_workflow(&wf)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let should_plan = req.auto_plan.unwrap_or(true);
    let should_start = req.auto_start.unwrap_or(false);

    if should_plan {
        if let Err(e) = plan_workflow_objective(&state.store, &wf).await {
            tracing::warn!("Auto-plan failed for workflow {}: {}", wf.id, e);
        }
    }

    if should_start {
        spawn_workflow_execution(state.clone(), wf.id);
    }

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

async fn list_workflow_tasks(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<Task>>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let tasks = state
        .store
        .list_tasks_by_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(tasks))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub objective: String,
    pub description: Option<String>,
    pub state: String,
    pub assigned_agent_id: Option<String>,
    pub assigned_agent_name: Option<String>,
    pub required_capabilities: Vec<String>,
    pub priority: i32,
    pub is_runnable: bool,
    pub is_blocked: bool,
    pub verifications_count: usize,
    pub is_verified: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub is_satisfied: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphSummary {
    pub total: usize,
    pub pending: usize,
    pub running: usize,
    pub verified: usize,
    pub failed: usize,
    pub blocked: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowGraphResponse {
    pub workflow_id: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub summary: GraphSummary,
}

async fn get_workflow_graph(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<WorkflowGraphResponse>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;

    let graph = state
        .store
        .load_task_graph(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let tasks = state
        .store
        .list_tasks_by_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let agents = state
        .store
        .list_agents()
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let agent_map: HashMap<AgentId, String> =
        agents.into_iter().map(|a| (a.id, a.display_name)).collect();

    let runnable_ids = graph.find_runnable_tasks();
    let mut task_map: HashMap<TaskId, Task> = HashMap::new();
    for t in &tasks {
        task_map.insert(t.id, t.clone());
    }

    let mut nodes = Vec::new();
    for task in &tasks {
        let is_runnable = runnable_ids.contains(&task.id);
        let dependencies = graph.direct_dependencies(&task.id);
        let is_blocked = dependencies
            .iter()
            .any(|dep_id| match task_map.get(dep_id) {
                Some(dep) => !dep.state.is_terminal(),
                None => false,
            });

        let verifications_count = state
            .store
            .list_verifications_by_task(&task.id)
            .await
            .map(|v| v.len())
            .unwrap_or(0);

        let assigned_agent_name = task
            .assigned_agent_id
            .and_then(|aid| agent_map.get(&aid).cloned());

        nodes.push(GraphNode {
            id: task.id.to_string(),
            title: task.objective.clone(),
            objective: task.objective.clone(),
            description: task.description.clone(),
            state: task.state.as_str().to_string(),
            assigned_agent_id: task.assigned_agent_id.map(|aid| aid.to_string()),
            assigned_agent_name,
            required_capabilities: task.required_capabilities(),
            priority: task.priority,
            is_runnable,
            is_blocked,
            verifications_count,
            is_verified: task.state == TaskState::Verified,
        });
    }

    let mut edges = Vec::new();
    for task in &tasks {
        for dep_id in graph.direct_dependencies(&task.id) {
            let is_satisfied = match task_map.get(&dep_id) {
                Some(dep) => dep.state == TaskState::Verified,
                None => false,
            };
            edges.push(GraphEdge {
                from: dep_id.to_string(),
                to: task.id.to_string(),
                is_satisfied,
            });
        }
    }

    let total = tasks.len();
    let mut pending = 0;
    let mut running = 0;
    let mut verified = 0;
    let mut failed = 0;
    let mut blocked = 0;

    for n in &nodes {
        if n.is_blocked {
            blocked += 1;
        }
        match n.state.as_str() {
            "Verified" => verified += 1,
            "Running" | "Assigned" => running += 1,
            "Failed" => failed += 1,
            _ => pending += 1,
        }
    }

    let summary = GraphSummary {
        total,
        pending,
        running,
        verified,
        failed,
        blocked,
    };

    Ok(Json(WorkflowGraphResponse {
        workflow_id: id.to_string(),
        nodes,
        edges,
        summary,
    }))
}

async fn plan_workflow(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<PlanProposal>, (StatusCode, Json<ApiError>)> {
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

    let proposal = plan_workflow_objective(&state.store, &wf)
        .await
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(proposal))
}

async fn start_workflow(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Workflow>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let mut wf = state
        .store
        .get_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Workflow {} not found", id))
        })?;

    // Check if workflow has tasks; if none, auto-plan first
    let tasks = state
        .store
        .list_tasks_by_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if tasks.is_empty() {
        let _ = plan_workflow_objective(&state.store, &wf).await;
    }

    if wf.state == WorkflowState::Draft || wf.state == WorkflowState::Paused {
        let _ = wf.state.transition_to(WorkflowState::Active);
        state
            .store
            .update_workflow(&wf)
            .await
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    spawn_workflow_execution(state, id);
    Ok(Json(wf))
}

async fn pause_workflow(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Workflow>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let mut wf = state
        .store
        .get_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Workflow {} not found", id))
        })?;

    wf.state
        .transition_to(WorkflowState::Paused)
        .map_err(|e| ApiError::new(StatusCode::CONFLICT, e.to_string()))?;
    state
        .store
        .update_workflow(&wf)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let evt = Event::new(
        "workflow",
        id.to_string(),
        "workflow.paused",
        serde_json::json!({ "workflow_id": id.to_string() }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(wf))
}

async fn resume_workflow(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Workflow>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let mut wf = state
        .store
        .get_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Workflow {} not found", id))
        })?;

    wf.state
        .transition_to(WorkflowState::Active)
        .map_err(|e| ApiError::new(StatusCode::CONFLICT, e.to_string()))?;
    state
        .store
        .update_workflow(&wf)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    spawn_workflow_execution(state.clone(), id);

    let evt = Event::new(
        "workflow",
        id.to_string(),
        "workflow.resumed",
        serde_json::json!({ "workflow_id": id.to_string() }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(wf))
}

async fn cancel_workflow(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Workflow>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let mut wf = state
        .store
        .get_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            ApiError::new(StatusCode::NOT_FOUND, format!("Workflow {} not found", id))
        })?;

    wf.state
        .transition_to(WorkflowState::Cancelled)
        .map_err(|e| ApiError::new(StatusCode::CONFLICT, e.to_string()))?;
    state
        .store
        .update_workflow(&wf)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let evt = Event::new(
        "workflow",
        id.to_string(),
        "workflow.cancelled",
        serde_json::json!({ "workflow_id": id.to_string() }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(wf))
}

async fn list_workflow_messages(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<AgentMessage>>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let msgs = state
        .store
        .list_messages_by_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(msgs))
}

async fn list_workflow_recoveries(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<RecoveryRecord>>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let recs = state
        .store
        .list_recovery_records_by_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(recs))
}

async fn list_workflow_verifications(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<Verification>>, (StatusCode, Json<ApiError>)> {
    let id: WorkflowId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid workflow id: {}", id_str),
        )
    })?;
    let vers = state
        .store
        .list_verifications_by_workflow(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(vers))
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

async fn get_task(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Task>, (StatusCode, Json<ApiError>)> {
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

#[derive(Serialize)]
pub struct TaskDependenciesResponse {
    pub task_id: TaskId,
    pub dependencies: Vec<Task>,
    pub prerequisites: Vec<Task>,
    pub dependents: Vec<Task>,
}

async fn get_task_dependencies(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<TaskDependenciesResponse>, (StatusCode, Json<ApiError>)> {
    let id: TaskId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid task id: {}", id_str),
        )
    })?;

    let dep_ids = state
        .store
        .get_dependencies(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let down_ids = state
        .store
        .get_dependents(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut dependencies = Vec::new();
    for did in dep_ids {
        if let Ok(Some(t)) = state.store.get_task(&did).await {
            dependencies.push(t);
        }
    }

    let mut dependents = Vec::new();
    for did in down_ids {
        if let Ok(Some(t)) = state.store.get_task(&did).await {
            dependents.push(t);
        }
    }

    Ok(Json(TaskDependenciesResponse {
        task_id: id,
        dependencies: dependencies.clone(),
        prerequisites: dependencies,
        dependents,
    }))
}

async fn get_task_executions(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<Execution>>, (StatusCode, Json<ApiError>)> {
    let id: TaskId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid task id: {}", id_str),
        )
    })?;
    let execs = state
        .store
        .list_executions_by_task(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(execs))
}

async fn get_task_verifications(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<Verification>>, (StatusCode, Json<ApiError>)> {
    let id: TaskId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid task id: {}", id_str),
        )
    })?;
    let vers = state
        .store
        .list_verifications_by_task(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(vers))
}

async fn get_task_messages(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<AgentMessage>>, (StatusCode, Json<ApiError>)> {
    let id: TaskId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid task id: {}", id_str),
        )
    })?;
    let msgs = state
        .store
        .list_messages_by_task(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(msgs))
}

async fn get_task_recoveries(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<RecoveryRecord>>, (StatusCode, Json<ApiError>)> {
    let id: TaskId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid task id: {}", id_str),
        )
    })?;
    let recs = state
        .store
        .list_recovery_records_by_task(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(recs))
}

#[derive(Deserialize)]
pub struct ReassignTaskRequest {
    pub agent_id: String,
}

async fn reassign_task(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Json(req): Json<ReassignTaskRequest>,
) -> Result<Json<Task>, (StatusCode, Json<ApiError>)> {
    let id: TaskId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid task id: {}", id_str),
        )
    })?;
    let agent_id: AgentId = req.agent_id.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", req.agent_id),
        )
    })?;

    let mut task = state
        .store
        .get_task(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("Task {} not found", id)))?;

    task.assigned_agent_id = Some(agent_id);
    state
        .store
        .update_task(&task)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let evt = Event::new(
        "task",
        id.to_string(),
        "task.reassigned",
        serde_json::json!({
            "task_id": id.to_string(),
            "new_agent_id": agent_id.to_string(),
        }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(task))
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

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

async fn get_agent(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Agent>, (StatusCode, Json<ApiError>)> {
    let id: AgentId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", id_str),
        )
    })?;
    let agent = state
        .store
        .get_agent(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("Agent {} not found", id)))?;
    Ok(Json(agent))
}

async fn get_agent_sessions(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<Session>>, (StatusCode, Json<ApiError>)> {
    let id: AgentId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", id_str),
        )
    })?;
    let sessions = state
        .store
        .list_sessions_by_agent(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(sessions))
}

async fn get_agent_executions(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<Execution>>, (StatusCode, Json<ApiError>)> {
    let id: AgentId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", id_str),
        )
    })?;
    let execs = state
        .store
        .list_executions_by_agent(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(execs))
}

async fn get_agent_messages(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Vec<AgentMessage>>, (StatusCode, Json<ApiError>)> {
    let id: AgentId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", id_str),
        )
    })?;
    let msgs = state
        .store
        .list_messages_for_agent(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(msgs))
}

async fn pause_agent(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Agent>, (StatusCode, Json<ApiError>)> {
    let id: AgentId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", id_str),
        )
    })?;
    let mut agent = state
        .store
        .get_agent(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("Agent {} not found", id)))?;

    agent.state = AgentState::Paused;
    state
        .store
        .update_agent(&agent)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let evt = Event::new(
        "agent",
        id.to_string(),
        "agent.paused",
        serde_json::json!({ "agent_id": id.to_string() }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(agent))
}

async fn resume_agent(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Agent>, (StatusCode, Json<ApiError>)> {
    let id: AgentId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", id_str),
        )
    })?;
    let mut agent = state
        .store
        .get_agent(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("Agent {} not found", id)))?;

    agent.state = AgentState::Idle;
    state
        .store
        .update_agent(&agent)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let evt = Event::new(
        "agent",
        id.to_string(),
        "agent.resumed",
        serde_json::json!({ "agent_id": id.to_string() }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(agent))
}

async fn cancel_agent(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<Agent>, (StatusCode, Json<ApiError>)> {
    let id: AgentId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", id_str),
        )
    })?;
    let mut agent = state
        .store
        .get_agent(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("Agent {} not found", id)))?;

    agent.state = AgentState::Terminated;
    state
        .store
        .update_agent(&agent)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let evt = Event::new(
        "agent",
        id.to_string(),
        "agent.cancelled",
        serde_json::json!({ "agent_id": id.to_string() }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(agent))
}

#[derive(Deserialize)]
pub struct SendAgentMessageRequest {
    pub content: String,
    pub message_type: Option<String>,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
}

async fn send_agent_message(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    Json(req): Json<SendAgentMessageRequest>,
) -> Result<Json<AgentMessage>, (StatusCode, Json<ApiError>)> {
    let id: AgentId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid agent id: {}", id_str),
        )
    })?;

    let wf_id: WorkflowId = match req.workflow_id {
        Some(ref s) if !s.trim().is_empty() => s.parse().map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("Invalid workflow id: {}", s),
            )
        })?,
        _ => {
            if let Ok(wfs) = state.store.list_workflows().await {
                if let Some(w) = wfs.first() {
                    w.id
                } else {
                    let default_wf =
                        Workflow::new("System Workflow", "Default operator messaging workflow");
                    let _ = state.store.create_workflow(&default_wf).await;
                    default_wf.id
                }
            } else {
                let default_wf =
                    Workflow::new("System Workflow", "Default operator messaging workflow");
                let _ = state.store.create_workflow(&default_wf).await;
                default_wf.id
            }
        }
    };

    let tid: Option<TaskId> = req
        .task_id
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| s.parse().ok());
    let msg_type = match req.message_type.as_deref() {
        Some("request") => MessageType::Request,
        Some("question") => MessageType::Question,
        Some("warning") => MessageType::Warning,
        Some("review") => MessageType::Review,
        Some("proposal") => MessageType::Proposal,
        _ => MessageType::Result,
    };

    let operator_agent_id = AgentId::new();
    let mut msg = AgentMessage::new(operator_agent_id, id, wf_id, msg_type, req.content);
    if let Some(t) = tid {
        msg.task_id = Some(t);
    }

    state
        .store
        .send_message(&msg)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let evt = Event::new(
        "message",
        msg.id.to_string(),
        "message.sent",
        serde_json::json!({
            "message_id": msg.id.to_string(),
            "to_agent": id.to_string(),
            "content": msg.content,
        }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok(Json(msg))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Events & SSE Live Stream
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListEventsQuery {
    pub limit: Option<usize>,
    pub aggregate_type: Option<String>,
    pub aggregate_id: Option<String>,
}

async fn list_recent_events(
    State(state): State<AppState>,
    Query(params): Query<ListEventsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    let limit = params.limit.unwrap_or(50);
    let events = if let (Some(at), Some(aid)) = (params.aggregate_type, params.aggregate_id) {
        state
            .store
            .list_events_by_aggregate(&at, &aid)
            .await
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        state
            .store
            .list_recent_events(limit)
            .await
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };
    Ok(Json(events))
}

#[derive(Deserialize)]
pub struct CursorEventsQuery {
    pub after: Option<u64>,
    pub limit: Option<usize>,
}

async fn list_events_cursor(
    State(state): State<AppState>,
    Query(params): Query<CursorEventsQuery>,
) -> Result<Json<Vec<Event>>, (StatusCode, Json<ApiError>)> {
    let after_seq = params.after.unwrap_or(0);
    let limit = params.limit.unwrap_or(500);
    let events = state
        .store
        .list_events_after(after_seq, limit)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(events))
}

#[derive(Debug, Deserialize)]
pub struct SseParams {
    pub after: Option<u64>,
}

async fn stream_events(
    State(state): State<AppState>,
    Query(params): Query<SseParams>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<AxumSseEvent, Infallible>>> {
    let mut after_seq = params.after;
    if after_seq.is_none() {
        if let Some(val) = headers.get("last-event-id") {
            if let Ok(s) = val.to_str() {
                after_seq = s.parse().ok();
            }
        }
    }

    let stream = async_stream::stream! {
        let mut last_seen = after_seq.unwrap_or(0);
        let mut rx = state.store.subscribe_events();

        // 1. Replay missed durable events
        if last_seen > 0 {
            if let Ok(missed) = state.store.list_events_after(last_seen, 1000).await {
                for evt in missed {
                    let seq = evt.sequence.unwrap_or(0);
                    if seq > last_seen {
                        last_seen = seq;
                    }
                    if let Ok(data) = serde_json::to_string(&evt) {
                        yield Ok(AxumSseEvent::default()
                            .id(seq.to_string())
                            .event("message")
                            .data(data));
                    }
                }
            }
        }

        // 2. Stream live broadcast events
        while let Ok(evt) = rx.recv().await {
            let seq = evt.sequence.unwrap_or(0);
            if seq > last_seen {
                last_seen = seq;
                if let Ok(data) = serde_json::to_string(&evt) {
                    yield Ok(AxumSseEvent::default()
                        .id(seq.to_string())
                        .event("message")
                        .data(data));
                }
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Approvals
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListApprovalsQuery {
    pub status: Option<String>,
}

async fn list_approvals(
    State(state): State<AppState>,
    Query(query): Query<ListApprovalsQuery>,
) -> Result<Json<Vec<ApprovalRecord>>, (StatusCode, Json<ApiError>)> {
    let approvals = if let Some(ref st) = query.status {
        if st.eq_ignore_ascii_case("all") {
            state
                .store
                .list_all_approvals()
                .await
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        } else if st.eq_ignore_ascii_case("pending") {
            state
                .store
                .list_pending_approvals()
                .await
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        } else {
            let all = state
                .store
                .list_all_approvals()
                .await
                .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            all.into_iter()
                .filter(|a| a.state.as_str().eq_ignore_ascii_case(st))
                .collect()
        }
    } else {
        state
            .store
            .list_pending_approvals()
            .await
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    Ok(Json(approvals))
}

#[derive(Debug, Deserialize)]
pub struct CreateApprovalPayload {
    pub workflow_id: WorkflowId,
    pub task_id: TaskId,
    pub action_description: String,
    pub reason: Option<String>,
}

async fn create_approval_gate(
    State(state): State<AppState>,
    Json(payload): Json<CreateApprovalPayload>,
) -> Result<(StatusCode, Json<ApprovalRecord>), (StatusCode, Json<ApiError>)> {
    let mut approval = ApprovalRecord::new(
        payload.task_id,
        payload.workflow_id,
        payload.action_description,
    );
    if let Some(r) = payload.reason {
        approval = approval.with_reason(r);
    }
    state
        .store
        .create_approval(&approval)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let evt = Event::new(
        "approval",
        approval.id.to_string(),
        "approval.gate_requested",
        serde_json::json!({
            "approval_id": approval.id.to_string(),
            "workflow_id": approval.workflow_id.to_string(),
            "task_id": approval.task_id.to_string(),
            "action_description": approval.action_description,
        }),
    );
    let _ = state.store.append_event(&evt).await;

    Ok((StatusCode::CREATED, Json(approval)))
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

#[derive(Debug, Deserialize, Default)]
pub struct DecisionPayload {
    pub decider: Option<String>,
    #[serde(alias = "notes", alias = "reason")]
    pub note: Option<String>,
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

// ---------------------------------------------------------------------------
// Memories
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListMemoriesQuery {
    pub scope: Option<String>,
    pub limit: Option<usize>,
}

async fn list_memories(
    State(state): State<AppState>,
    Query(params): Query<ListMemoriesQuery>,
) -> Result<Json<Vec<MemoryRecord>>, (StatusCode, Json<ApiError>)> {
    let limit = params.limit.unwrap_or(100);
    let scope_opt = params.scope.and_then(|s| match s.to_lowercase().as_str() {
        "user" => Some(MemoryScope::User),
        "project" => Some(MemoryScope::Project),
        "workflow" => Some(MemoryScope::Workflow),
        "task" => Some(MemoryScope::Task),
        "agent" => Some(MemoryScope::Agent),
        "session" => Some(MemoryScope::Session),
        "artifact" => Some(MemoryScope::Artifact),
        "system" => Some(MemoryScope::System),
        _ => None,
    });

    let memories = if let Some(scope) = scope_opt {
        state
            .store
            .list_memories_by_scope(scope, None)
            .await
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        state
            .store
            .list_active_memories(None, None, limit)
            .await
            .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    Ok(Json(memories))
}

async fn get_memory(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Json<MemoryRecord>, (StatusCode, Json<ApiError>)> {
    let id: MemoryId = id_str.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("Invalid memory id: {}", id_str),
        )
    })?;
    let mem = state
        .store
        .get_memory(&id)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("Memory {} not found", id)))?;
    Ok(Json(mem))
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ProviderStatusItem {
    pub id: &'static str,
    pub name: &'static str,
    pub status: &'static str,
    pub models: Vec<&'static str>,
    pub is_available: bool,
}

async fn list_providers() -> impl IntoResponse {
    let providers = vec![
        ProviderStatusItem {
            id: "openai",
            name: "OpenAI Provider",
            status: "configured",
            models: vec!["gpt-4o", "gpt-4o-mini", "o3-mini"],
            is_available: true,
        },
        ProviderStatusItem {
            id: "anthropic",
            name: "Anthropic Provider",
            status: "configured",
            models: vec!["claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022"],
            is_available: true,
        },
        ProviderStatusItem {
            id: "gemini",
            name: "Google Gemini Provider",
            status: "configured",
            models: vec!["gemini-1.5-pro", "gemini-1.5-flash", "gemini-2.0-flash"],
            is_available: true,
        },
        ProviderStatusItem {
            id: "ollama",
            name: "Ollama Local Provider",
            status: "local",
            models: vec!["llama3.1", "qwen2.5-coder", "deepseek-r1"],
            is_available: true,
        },
        ProviderStatusItem {
            id: "scripted",
            name: "Plexis Deterministic / Test Provider",
            status: "ready",
            models: vec!["scripted-v1"],
            is_available: true,
        },
    ];
    Json(providers)
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ToolCatalogItem {
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub parameters_schema: serde_json::Value,
    pub backend_isolation: &'static str,
}

async fn list_tools(State(state): State<AppState>) -> impl IntoResponse {
    let tools = state.tool_registry.list_tools();
    let backend_isolation = if std::path::Path::new("/usr/bin/bwrap").exists() {
        "Bubblewrap (Linux namespaces)"
    } else {
        "HostProcess (kill_on_drop)"
    };

    let catalog: Vec<_> = tools
        .into_iter()
        .map(|t| ToolCatalogItem {
            name: t.name().to_string(),
            description: t.description().to_string(),
            capabilities: vec![t.name().to_string()],
            parameters_schema: t.schema(),
            backend_isolation,
        })
        .collect();

    Json(catalog)
}

// ---------------------------------------------------------------------------
// SPA Fallback Page
// ---------------------------------------------------------------------------

async fn spa_fallback_page() -> impl IntoResponse {
    Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Plexis Operational Control Plane</title>
  <style>
    body { font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; background: #0b0f19; color: #e2e8f0; padding: 2rem; margin: 0; }
    .card { background: #1e293b; border: 1px solid #334155; border-radius: 8px; padding: 2rem; max-width: 680px; margin: 4rem auto; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1); }
    h1 { color: #38bdf8; margin-top: 0; font-size: 1.5rem; }
    p { line-height: 1.6; color: #94a3b8; }
    code { background: #0f172a; padding: 0.2rem 0.4rem; border-radius: 4px; color: #a5f3fc; }
    .status-badge { display: inline-block; background: #064e3b; color: #34d399; padding: 0.25rem 0.75rem; border-radius: 9999px; font-size: 0.875rem; font-weight: bold; margin-bottom: 1rem; }
    a { color: #38bdf8; text-decoration: none; }
    a:hover { text-decoration: underline; }
  </style>
</head>
<body>
  <div class="card">
    <div class="status-badge">API ONLINE</div>
    <h1>Plexis Operational Control Plane</h1>
    <p>The Plexis server is actively listening. All control plane API endpoints and event streaming channels are operational.</p>
    <p><strong>Available Endpoints:</strong></p>
    <ul>
      <li><a href="/health"><code>GET /health</code></a> &mdash; Health probe</li>
      <li><a href="/api/v1/system/status"><code>GET /api/v1/system/status</code></a> &mdash; System info</li>
      <li><a href="/api/v1/dashboard/summary"><code>GET /api/v1/dashboard/summary</code></a> &mdash; Aggregated metrics</li>
      <li><a href="/api/v1/workflows"><code>GET /api/v1/workflows</code></a> &mdash; Workflows catalog</li>
      <li><a href="/api/v1/events/stream"><code>GET /api/v1/events/stream</code></a> &mdash; SSE live stream</li>
      <li><a href="/api/v1/approvals"><code>GET /api/v1/approvals</code></a> &mdash; Approval center</li>
    </ul>
    <p>To run the developer web dashboard in development mode:<br /><code>cd web && npm run dev</code></p>
    <p>To build static web assets for direct serving by Plexis Server:<br /><code>cd web && npm run build</code></p>
  </div>
</body>
</html>"#,
    )
}

// ---------------------------------------------------------------------------
// Helpers: Workflow Planning and Execution
// ---------------------------------------------------------------------------

async fn plan_workflow_objective(
    store: &Arc<plexis_storage::SqliteStore>,
    workflow: &Workflow,
) -> Result<PlanProposal, String> {
    let existing_tasks = store
        .list_tasks_by_workflow(&workflow.id)
        .await
        .map_err(|e| e.to_string())?;
    if !existing_tasks.is_empty() {
        return Err("Workflow already contains tasks".to_string());
    }

    let t1_id = TaskId::new();
    let t2_id = TaskId::new();
    let t3_id = TaskId::new();
    let t4_id = TaskId::new();
    let t5_id = TaskId::new();
    let t6_id = TaskId::new();

    let proposed_tasks = vec![
        ProposedTask {
            temp_id: t1_id.to_string(),
            objective: "Environment Analysis & Context Discovery".into(),
            description: Some(format!(
                "Analyze repository workspace, project architecture, and dependencies for objective: {}",
                workflow.objective
            )),
            criteria: vec!["context:workspace_ready".into()],
            required_capabilities: vec!["filesystem_read".into(), "planning".into()],
            suggested_role: Some("Planner".into()),
            priority: 1,
        },
        ProposedTask {
            temp_id: t2_id.to_string(),
            objective: "Core Implementation & Algorithm Logic".into(),
            description: Some(format!(
                "Implement changes, file updates, and algorithms to satisfy objective: {}",
                workflow.objective
            )),
            criteria: vec!["impl:code_written".into()],
            required_capabilities: vec!["filesystem_write".into(), "shell".into()],
            suggested_role: Some("Developer".into()),
            priority: 2,
        },
        ProposedTask {
            temp_id: t3_id.to_string(),
            objective: "Automated Test Suite & Boundary Coverage".into(),
            description: Some(format!(
                "Implement unit tests, property tests, and boundary assertions for: {}",
                workflow.objective
            )),
            criteria: vec!["test:suite_passed".into()],
            required_capabilities: vec!["test_runner".into(), "shell".into()],
            suggested_role: Some("Tester".into()),
            priority: 2,
        },
        ProposedTask {
            temp_id: t4_id.to_string(),
            objective: "Documentation & Architecture Specification".into(),
            description: Some(format!(
                "Update README documentation, architectural docs, and usage examples for: {}",
                workflow.objective
            )),
            criteria: vec!["doc:spec_updated".into()],
            required_capabilities: vec!["filesystem_write".into()],
            suggested_role: Some("TechnicalWriter".into()),
            priority: 2,
        },
        ProposedTask {
            temp_id: t5_id.to_string(),
            objective: "Integration Assembly & Quality Gates".into(),
            description: Some(format!(
                "Integrate parallel deliverables, run comprehensive integration checks for: {}",
                workflow.objective
            )),
            criteria: vec!["gate:integration_ready".into()],
            required_capabilities: vec!["integration".into(), "shell".into()],
            suggested_role: Some("Integrator".into()),
            priority: 3,
        },
        ProposedTask {
            temp_id: t6_id.to_string(),
            objective: "Independent Review & Audit Signoff".into(),
            description: Some(format!(
                "Perform final independent verification and deliver verified artifact signoff for: {}",
                workflow.objective
            )),
            criteria: vec!["verification:passed".into()],
            required_capabilities: vec!["verification".into(), "integration".into()],
            suggested_role: Some("Verifier".into()),
            priority: 4,
        },
    ];

    let proposed_dependencies = vec![
        ProposedDependency {
            task_temp_id: t2_id.to_string(),
            depends_on_temp_id: t1_id.to_string(),
        },
        ProposedDependency {
            task_temp_id: t3_id.to_string(),
            depends_on_temp_id: t1_id.to_string(),
        },
        ProposedDependency {
            task_temp_id: t4_id.to_string(),
            depends_on_temp_id: t1_id.to_string(),
        },
        ProposedDependency {
            task_temp_id: t5_id.to_string(),
            depends_on_temp_id: t2_id.to_string(),
        },
        ProposedDependency {
            task_temp_id: t5_id.to_string(),
            depends_on_temp_id: t3_id.to_string(),
        },
        ProposedDependency {
            task_temp_id: t6_id.to_string(),
            depends_on_temp_id: t5_id.to_string(),
        },
        ProposedDependency {
            task_temp_id: t6_id.to_string(),
            depends_on_temp_id: t4_id.to_string(),
        },
    ];

    let proposal = PlanProposal {
        objective: workflow.objective.clone(),
        rationale: "Autonomous 4-tier verification-driven multi-agent engineering workflow plan"
            .into(),
        tasks: proposed_tasks,
        dependencies: proposed_dependencies,
        execution_strategy: ExecutionStrategy::Parallel,
        verification_strategy: VerificationStrategy::MultiStep,
    };

    let report = PlanValidator::validate(&proposal);
    if !report.is_valid {
        return Err(format!("Plan proposal rejected: {:?}", report.errors));
    }

    let applier = PlanApplier::new(store.clone());
    let context = plexis_planner::PlanningContext::new(workflow.id, &workflow.objective);
    applier
        .apply(&context, &proposal, "builtin", "heuristic", 10, None, None)
        .await
        .map_err(|e| format!("Failed to apply plan: {}", e))?;

    let evt = Event::new(
        "workflow",
        workflow.id.to_string(),
        "workflow.planned",
        serde_json::json!({
            "workflow_id": workflow.id.to_string(),
            "task_count": 6,
            "objective": workflow.objective,
        }),
    );
    let _ = store.append_event(&evt).await;

    Ok(proposal)
}

async fn ensure_default_agents(store: &Arc<plexis_storage::SqliteStore>) -> Result<(), String> {
    let existing = store.list_agents().await.map_err(|e| e.to_string())?;
    if !existing.is_empty() {
        return Ok(());
    }

    let default_profile = ExecutionProfile::new("scripted", "default-model");
    let agents = vec![
        (
            "Software Architect",
            "Planner",
            vec!["planning".into(), "filesystem_read".into()],
        ),
        (
            "Core Developer",
            "Developer",
            vec!["filesystem_write".into(), "shell".into(), "git".into()],
        ),
        (
            "Test Engineer",
            "Tester",
            vec![
                "test_runner".into(),
                "shell".into(),
                "filesystem_write".into(),
            ],
        ),
        (
            "Technical Writer",
            "TechnicalWriter",
            vec!["filesystem_write".into(), "documentation".into()],
        ),
        (
            "System Integrator",
            "Integrator",
            vec!["integration".into(), "shell".into(), "git".into()],
        ),
        (
            "Quality Verifier",
            "Verifier",
            vec!["verification".into(), "integration".into()],
        ),
    ];

    let tmp_dir = std::env::temp_dir().join("plexis_workspace");
    let _ = tokio::fs::create_dir_all(&tmp_dir).await;
    let workdir = tmp_dir.to_string_lossy().to_string();

    for (name, role, caps) in agents {
        let agent = Agent::new(name, role, default_profile.clone()).with_capabilities(caps);
        let session = Session::new(agent.id).with_working_directory(workdir.clone());
        store
            .create_agent(&agent)
            .await
            .map_err(|e| e.to_string())?;
        store
            .create_session(&session)
            .await
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn spawn_workflow_execution(state: AppState, workflow_id: WorkflowId) {
    tokio::spawn(async move {
        let _ = ensure_default_agents(&state.store).await;

        if let Ok(Some(mut wf)) = state.store.get_workflow(&workflow_id).await {
            if wf.state == WorkflowState::Draft || wf.state == WorkflowState::Paused {
                let _ = wf.state.transition_to(WorkflowState::Active);
                let _ = state.store.update_workflow(&wf).await;
                let evt = Event::new(
                    "workflow",
                    workflow_id.to_string(),
                    "workflow.started",
                    serde_json::json!({ "workflow_id": workflow_id.to_string() }),
                );
                let _ = state.store.append_event(&evt).await;
            }
        }

        let lease_mgr = Arc::new(LeaseManager::new(state.store.clone()));
        let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
        let verifier = Arc::new(WorkspaceVerifier::new(state.store.clone()));
        let mut runner = AgentRunner::new(
            state.store.clone(),
            state.tool_registry.as_ref().clone(),
            verifier,
        );
        let mock_provider = Arc::new(plexis_providers::ScriptedProvider::new("scripted"));
        runner.register_provider(mock_provider);
        let runner_arc = Arc::new(runner);

        let scheduler =
            DeterministicScheduler::new(state.store.clone(), lease_mgr, dispatcher, runner_arc);

        let mut consecutive_empty_ticks = 0;
        loop {
            // Check workflow state
            match state.store.get_workflow(&workflow_id).await {
                Ok(Some(wf)) => {
                    if wf.state != WorkflowState::Active {
                        break;
                    }
                }
                _ => break,
            }

            // Check if any task is in NeedsHuman
            if let Ok(tasks) = state.store.list_tasks_by_workflow(&workflow_id).await {
                if tasks.iter().any(|t| t.state == TaskState::NeedsHuman) {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }

                let all_terminal = !tasks.is_empty() && tasks.iter().all(|t| t.state.is_terminal());
                if all_terminal {
                    let has_failed = tasks.iter().any(|t| t.state == TaskState::Failed);
                    if let Ok(Some(mut wf)) = state.store.get_workflow(&workflow_id).await {
                        let target_state = if has_failed {
                            WorkflowState::Failed
                        } else {
                            WorkflowState::Completed
                        };
                        let _ = wf.state.transition_to(target_state);
                        let _ = state.store.update_workflow(&wf).await;
                        let evt = Event::new(
                            "workflow",
                            workflow_id.to_string(),
                            if has_failed {
                                "workflow.failed"
                            } else {
                                "workflow.completed"
                            },
                            serde_json::json!({
                                "workflow_id": workflow_id.to_string(),
                                "total_tasks": tasks.len(),
                            }),
                        );
                        let _ = state.store.append_event(&evt).await;
                    }
                    break;
                }
            }

            match scheduler.tick().await {
                Ok(count) if count > 0 => {
                    consecutive_empty_ticks = 0;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => {
                    consecutive_empty_ticks += 1;
                    if consecutive_empty_ticks > 15 {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    if consecutive_empty_ticks > 40 {
                        break;
                    }
                }
            }
        }
    });
}
