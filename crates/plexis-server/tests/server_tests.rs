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
