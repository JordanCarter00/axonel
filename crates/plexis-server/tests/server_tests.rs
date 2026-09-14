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
