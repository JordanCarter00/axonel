use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use plexis_server::{create_router, AppState};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_health_check_endpoint() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_workflow_api_endpoints() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    // Create workflow via POST
    let create_payload = serde_json::json!({
        "title": "API Test Workflow",
        "objective": "Verify HTTP API"
    });

    let create_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/workflows")
                .header("content-type", "application/json")
                .body(Body::from(create_payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("create workflow");

    assert_eq!(create_res.status(), StatusCode::CREATED);
    let create_body = create_res.into_body().collect().await.unwrap().to_bytes();
    let created_json: serde_json::Value = serde_json::from_slice(&create_body).unwrap();
    let wf_id = created_json["id"].as_str().unwrap();

    // Fetch workflow via GET
    let get_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/workflows/{wf_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("get workflow");

    assert_eq!(get_res.status(), StatusCode::OK);
    let get_body = get_res.into_body().collect().await.unwrap().to_bytes();
    let get_json: serde_json::Value = serde_json::from_slice(&get_body).unwrap();
    assert_eq!(get_json["title"], "API Test Workflow");
}

#[tokio::test]
async fn test_workflow_validation_and_structured_error() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    // Empty title must yield 400 Bad Request with ApiError
    let bad_payload = serde_json::json!({
        "title": "   ",
        "objective": "Some objective"
    });

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/workflows")
                .header("content-type", "application/json")
                .body(Body::from(bad_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("title cannot be empty"));
    assert_eq!(json["status"], 400);
}

#[tokio::test]
async fn test_approval_api_endpoints() {
    use plexis_core::{ApprovalRecord, Task, Workflow};
    use plexis_storage::traits::{ApprovalStore, TaskStore, WorkflowStore};

    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let wf = Workflow::new("Approval WF", "Test approval endpoints");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Sensitive Deployment Task");
    store.create_task(&task).await.unwrap();

    let approval = ApprovalRecord::new(task.id, wf.id, "Deploy to production environment");
    store.create_approval(&approval).await.unwrap();

    let state = AppState::new(store);
    let app = create_router(state);

    // 1. List approvals
    let list_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/approvals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(list_res.status(), StatusCode::OK);
    let body = list_res.into_body().collect().await.unwrap().to_bytes();
    let list_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(list_json.as_array().unwrap().len(), 1);

    // 2. Approve gate
    let approve_payload = serde_json::json!({
        "decider": "admin",
        "note": "Approved after manual review"
    });

    let approve_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/approvals/{}/approve", approval.id))
                .header("content-type", "application/json")
                .body(Body::from(approve_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(approve_res.status(), StatusCode::OK);
    let approve_body = approve_res.into_body().collect().await.unwrap().to_bytes();
    let approved_json: serde_json::Value = serde_json::from_slice(&approve_body).unwrap();
    assert_eq!(approved_json["state"], "approved");
    assert_eq!(approved_json["reason"], "Approved after manual review");
}

#[tokio::test]
async fn test_dashboard_summary_endpoint() {
    use plexis_core::{Task, Workflow};
    use plexis_storage::traits::{TaskStore, WorkflowStore};

    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let wf = Workflow::new("Dashboard WF", "Check dashboard KPIs");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "KPI task");
    store.create_task(&task).await.unwrap();

    let state = AppState::new(store);
    let app = create_router(state);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/dashboard/summary")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["total_workflows"], 1);
    assert_eq!(json["active_workflows"], 1);
    assert!(json["workflows"].is_array());
    assert!(json["provider_health"].is_object());
}

#[tokio::test]
async fn test_workflow_graph_endpoint() {
    use plexis_core::{Task, Workflow};
    use plexis_storage::traits::{TaskStore, WorkflowStore};

    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let wf = Workflow::new("Graph Test WF", "Verify DAG layout endpoint");
    store.create_workflow(&wf).await.unwrap();

    let task_a = Task::new(wf.id, "Prerequisite Task A");
    let task_b = Task::new(wf.id, "Dependent Task B");

    store.create_task(&task_a).await.unwrap();
    store.create_task(&task_b).await.unwrap();
    store.add_dependency(&task_b.id, &task_a.id).await.unwrap();

    let state = AppState::new(store);
    let app = create_router(state);

    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/workflows/{}/graph", wf.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["workflow_id"], wf.id.to_string());
    assert_eq!(json["nodes"].as_array().unwrap().len(), 2);
    let edges = json["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["from"], task_a.id.to_string());
    assert_eq!(edges[0]["to"], task_b.id.to_string());
    assert_eq!(json["summary"]["total"], 2);
}

#[tokio::test]
async fn test_event_stream_and_reconnect_cursor() {
    use plexis_core::Event;
    use plexis_storage::traits::EventStore;

    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    // Append 3 events
    let e1 = Event::new(
        "workflow",
        "wf-1",
        "workflow.created",
        serde_json::json!({ "name": "WF1" }),
    );
    let e2 = Event::new(
        "task",
        "task-1",
        "task.created",
        serde_json::json!({ "title": "T1" }),
    );
    let e3 = Event::new(
        "workflow",
        "wf-1",
        "workflow.updated",
        serde_json::json!({ "state": "Executing" }),
    );

    store.append_event(&e1).await.unwrap();
    store.append_event(&e2).await.unwrap();
    store.append_event(&e3).await.unwrap();

    let state = AppState::new(store);
    let app = create_router(state);

    // 1. Fetch cursor replay after seq 1
    let res_cursor = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/events/cursor?after=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res_cursor.status(), StatusCode::OK);
    let body = res_cursor.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let events = json.as_array().unwrap();
    assert_eq!(events.len(), 2); // seq 2 and 3
    assert_eq!(events[1]["sequence"], 3);

    // 2. SSE stream headers check
    let sse_res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/events/stream?after=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(sse_res.status(), StatusCode::OK);
    let content_type = sse_res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream"));
}

#[tokio::test]
async fn test_spa_static_serving_fallback() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    let res = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&body);
    assert!(html.contains("<!doctype html>") || html.contains("<!DOCTYPE html>"));
}

#[tokio::test]
async fn test_autonomous_workflow_creation_and_api_lifecycle() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    // 1. Create autonomous workflow with auto_plan = true and auto_start = true
    let payload = serde_json::json!({
        "title": "Autonomous Cache Feature",
        "objective": "Design, implement and test LRU cache with concurrency",
        "auto_plan": true,
        "auto_start": true
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/workflows")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let wf_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let wf_id = wf_json["id"].as_str().unwrap();

    // 2. Fetch graph representation
    let graph_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/workflows/{}/graph", wf_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(graph_res.status(), StatusCode::OK);
    let graph_body = graph_res.into_body().collect().await.unwrap().to_bytes();
    let graph_json: serde_json::Value = serde_json::from_slice(&graph_body).unwrap();
    assert_eq!(graph_json["workflow_id"], wf_id);
    assert!(!graph_json["nodes"].as_array().unwrap().is_empty());

    // 3. Inspect task dependencies for the first node
    let first_task_id = graph_json["nodes"][0]["id"].as_str().unwrap();
    let dep_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/tasks/{}/dependencies", first_task_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(dep_res.status(), StatusCode::OK);

    // 4. Verify dashboard summary updates
    let dash_res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/dashboard/summary")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(dash_res.status(), StatusCode::OK);
    let dash_body = dash_res.into_body().collect().await.unwrap().to_bytes();
    let dash_json: serde_json::Value = serde_json::from_slice(&dash_body).unwrap();
    assert_eq!(dash_json["total_workflows"], 1);
}

#[tokio::test]
async fn test_workspace_api_crud_and_git_status() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    // 1. Create workspace
    let temp_dir = std::env::temp_dir().join("plexis_ws_test");
    let _ = std::fs::create_dir_all(&temp_dir);

    let create_payload = serde_json::json!({
        "name": "Test Project Workspace",
        "canonical_path": temp_dir.to_string_lossy(),
        "description": "Integration testing workspace",
        "is_default": true
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/workspaces")
                .header("content-type", "application/json")
                .body(Body::from(create_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let ws_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let ws_id = ws_json["id"].as_str().unwrap();
    assert_eq!(ws_json["name"], "Test Project Workspace");

    // 2. List workspaces
    let list_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/workspaces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_res.status(), StatusCode::OK);
    let list_body = list_res.into_body().collect().await.unwrap().to_bytes();
    let list_json: serde_json::Value = serde_json::from_slice(&list_body).unwrap();
    assert_eq!(list_json.as_array().unwrap().len(), 1);

    // 3. Get workspace by id
    let get_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/workspaces/{}", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_res.status(), StatusCode::OK);

    // 4. Git status check
    let git_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/workspaces/{}/git/status", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(git_res.status(), StatusCode::OK);

    // 5. Delete workspace
    let del_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/workspaces/{}", ws_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_provider_capabilities_endpoint() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/providers/capabilities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array());
    assert!(!json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_task_terminal_streaming_and_redaction() {
    use plexis_core::{Task, Workflow};
    use plexis_storage::traits::{TaskStore, WorkflowStore};

    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let workflow = Workflow::new("Terminal Test Workflow", "Test terminal streaming");
    let wf_id = workflow.id;
    store.create_workflow(&workflow).await.unwrap();

    let task = Task::new(wf_id, "Terminal test task");
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    let state = AppState::new(store);
    // Append simulated output with API key to test redaction
    state.terminal_buffer.append(
        &task_id,
        "stdout",
        "Exporting sk-test1234567890abcdef1234567890abcdef for deployment",
    );
    state.terminal_buffer.complete(&task_id, 0);

    let app = create_router(state);

    let res = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/tasks/{}/terminal", task_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["task_id"], task_id.to_string());
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["is_completed"], true);

    let line = json["lines"][0]["line"].as_str().unwrap();
    assert!(!line.contains("sk-test1234567890abcdef1234567890abcdef"));
    assert!(line.contains("[REDACTED_API_KEY]"));
}

#[tokio::test]
async fn test_retention_prune_endpoint() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    let payload = serde_json::json!({
        "max_age_days": 14
    });

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/retention/prune")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("records_pruned").is_some());
    assert!(json.get("cutoff_date").is_some());
}

#[tokio::test]
async fn test_hardened_token_auth_middleware() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store).with_auth_token(Some("secure_token_xyz".into()));
    let app = create_router(state);

    // 1. Public route /health and /api/v1/health succeed without token
    let health_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health_res.status(), StatusCode::OK);

    let api_health_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(api_health_res.status(), StatusCode::OK);

    // 2. Auth status reports auth is enabled
    let auth_status_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(auth_status_res.status(), StatusCode::OK);

    // 3. Protected route without token returns 401 Unauthorized
    let unauth_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/workspaces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth_res.status(), StatusCode::UNAUTHORIZED);

    // 4. Protected route with invalid token returns 401 Unauthorized
    let invalid_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/workspaces")
                .header("authorization", "Bearer wrong_token_abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid_res.status(), StatusCode::UNAUTHORIZED);

    // 5. Protected route with Bearer header succeeds
    let bearer_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/workspaces")
                .header("authorization", "Bearer secure_token_xyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bearer_res.status(), StatusCode::OK);

    // 6. Protected route with query parameter ?token= succeeds (SSE compatibility)
    let query_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/workspaces?token=secure_token_xyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(query_res.status(), StatusCode::OK);

    // 7. Token rotation support: multi-token configuration permits both new and old tokens
    let rotation_store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let rotation_state = AppState::new(rotation_store)
        .with_auth_token(Some("active_new_token,legacy_old_token".into()));
    let rotation_app = create_router(rotation_state);

    let new_tok_res = rotation_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/workspaces")
                .header("authorization", "Bearer active_new_token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(new_tok_res.status(), StatusCode::OK);

    let old_tok_res = rotation_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/workspaces")
                .header("authorization", "Bearer legacy_old_token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_tok_res.status(), StatusCode::OK);

    let bad_tok_res = rotation_app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/workspaces")
                .header("authorization", "Bearer revoked_token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_tok_res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_github_integration_endpoints() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state);

    // 1. List repos
    let repos_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/github/repos")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(repos_res.status(), StatusCode::OK);

    // 2. List PRs
    let prs_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/github/pulls?repo=plexis")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(prs_res.status(), StatusCode::OK);

    // 3. Create PR with repo and workflow link
    let pr_payload = serde_json::json!({
        "repo": "axonel/axonel",
        "title": "feat: add capability matrix",
        "body": "Implements provider capabilities and reasoning tiers",
        "head": "feat/capabilities",
        "base": "main",
        "workflow_id": "wf-1234",
        "task_id": "task-5678"
    });

    let create_pr_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/github/pulls")
                .header("content-type", "application/json")
                .body(Body::from(pr_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_pr_res.status(), StatusCode::OK);
    let pr_body = create_pr_res
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let pr_json: serde_json::Value = serde_json::from_slice(&pr_body).unwrap();
    assert_eq!(pr_json["title"], "feat: add capability matrix");

    // 4. List Issues
    let issues_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/github/issues?repo=axonel/axonel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(issues_res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_agent_host_api_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let ws_dir = temp.path().join("workspace");
    std::fs::create_dir_all(ws_dir.join("src")).unwrap();
    std::fs::write(
        ws_dir.join("Cargo.toml"),
        "[package]\nname = \"test_pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        ws_dir.join("src/lib.rs"),
        "pub fn compute(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store).with_auth_token(Some("test-secret-token-123".to_string()));
    let app = create_router(state);

    // 1. Unauthenticated request rejected with 401
    let unauth_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/agent-host/backends")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth_res.status(), StatusCode::UNAUTHORIZED);

    // 2. Authenticated GET /api/v1/agent-host/backends
    let backends_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/agent-host/backends")
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(backends_res.status(), StatusCode::OK);
    let body = backends_res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let backends = json["backends"].as_array().expect("backends array");
    assert!(backends.iter().any(|b| b["id"] == "fake_agent"));

    // 3. Authenticated POST /api/v1/agent-host/executions
    let exec_payload = serde_json::json!({
        "workspace_path": ws_dir.to_string_lossy().to_string(),
        "objective": "Test external execution",
        "backend": "fake_agent",
        "timeout_secs": 30
    });

    let exec_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/agent-host/executions")
                .header("authorization", "Bearer test-secret-token-123")
                .header("content-type", "application/json")
                .body(Body::from(exec_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(exec_res.status(), StatusCode::OK);
    let exec_body = exec_res.into_body().collect().await.unwrap().to_bytes();
    let exec_json: serde_json::Value = serde_json::from_slice(&exec_body).unwrap();
    assert_eq!(exec_json["exit_code"], 0);
    assert_eq!(exec_json["success"], true);

    // 4. Authenticated GET /api/v1/agent-host/executions
    let list_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/agent-host/executions")
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_mission_control_api_endpoints() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store).with_auth_token(Some("test-secret-token-123".into()));
    let app = create_router(state);

    // 1. Create Mission via POST /api/v1/missions
    let create_payload = serde_json::json!({
        "title": "Long Horizon Refactor Mission",
        "objective": "Safely refactor core types across multiple cycles",
        "budget": {
            "max_duration_secs": 3600,
            "max_concurrent_agents": 2,
            "max_executions": 10,
            "max_recovery_attempts": 3,
            "max_planner_iterations": 5,
            "max_stagnant_cycles": 3
        },
        "stopping_condition": {
            "required_tests_pass": false,
            "working_tree_clean": false,
            "required_commit_exists": false
        },
        "auto_start": false
    });

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/missions")
                .header("authorization", "Bearer test-secret-token-123")
                .header("content-type", "application/json")
                .body(Body::from(create_payload.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let mission_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let mission_id = mission_json["id"].as_str().unwrap().to_string();
    assert_eq!(mission_json["state"], "created");

    // 2. GET /api/v1/missions
    let list_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/missions")
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_res.status(), StatusCode::OK);
    let list_body = list_res.into_body().collect().await.unwrap().to_bytes();
    let list_json: Vec<serde_json::Value> = serde_json::from_slice(&list_body).unwrap();
    assert_eq!(list_json.len(), 1);

    // 3. GET /api/v1/missions/{id}
    let get_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/missions/{}", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_res.status(), StatusCode::OK);

    // 4. POST /api/v1/missions/{id}/start
    let start_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/missions/{}/start", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_res.status(), StatusCode::OK);
    let start_json: serde_json::Value = serde_json::from_slice(
        &start_res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(start_json["state"], "planning");

    // 5. POST /api/v1/missions/{id}/pause
    let pause_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/missions/{}/pause", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pause_res.status(), StatusCode::OK);

    // 6. POST /api/v1/missions/{id}/resume
    let resume_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/missions/{}/resume", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resume_res.status(), StatusCode::OK);

    // 7. POST /api/v1/missions/{id}/escalate
    let esc_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/missions/{}/escalate", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "reason": "Operator review needed" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(esc_res.status(), StatusCode::OK);
    let esc_json: serde_json::Value =
        serde_json::from_slice(&esc_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(esc_json["state"], "needs_human");

    // 8. POST /api/v1/missions/{id}/resolve
    let resolve_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/missions/{}/resolve", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "decision": "replan" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resolve_res.status(), StatusCode::OK);
    let resolve_json: serde_json::Value = serde_json::from_slice(
        &resolve_res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert_eq!(resolve_json["state"], "replanning");

    // 9. GET /api/v1/missions/{id}/status
    let status_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/missions/{}/status", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status_res.status(), StatusCode::OK);
    let status_json: serde_json::Value = serde_json::from_slice(
        &status_res.into_body().collect().await.unwrap().to_bytes(),
    )
    .unwrap();
    assert!(status_json.get("mission").is_some());

    // 10. GET /api/v1/missions/{id}/events
    let ev_res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/missions/{}/events", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ev_res.status(), StatusCode::OK);
    let ev_json: Vec<serde_json::Value> =
        serde_json::from_slice(&ev_res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(!ev_json.is_empty());

    // 11. POST /api/v1/missions/{id}/cancel
    let cancel_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/missions/{}/cancel", mission_id))
                .header("authorization", "Bearer test-secret-token-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(cancel_res.status(), StatusCode::OK);
}
