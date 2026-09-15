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
