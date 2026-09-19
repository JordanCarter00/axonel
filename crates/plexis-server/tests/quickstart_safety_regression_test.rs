use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::process::Command;
use tempfile::tempdir;
use tower::ServiceExt;

use plexis_server::{create_router, AppState};
use plexis_storage::SqliteStore;

fn setup_repo(dir: &std::path::Path) -> String {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {:?}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };

    run(&["init", "-b", "main"]);
    run(&["config", "user.name", "Test User"]);
    run(&["config", "user.email", "test@axonel.local"]);

    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        "pub fn hello() -> &'static str { \"hello\" }\n",
    )
    .unwrap();
    run(&["add", "Cargo.toml", "src/lib.rs"]);
    run(&["commit", "-m", "Initial baseline commit"]);

    let head_out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&head_out.stdout).trim().to_string()
}

#[tokio::test]
async fn test_quickstart_never_commits_to_main_or_sweeps_untracked() {
    let temp_repo = tempdir().unwrap();
    let initial_sha = setup_repo(temp_repo.path());

    // Create untracked files that must never be swept up by git add
    std::fs::write(temp_repo.path().join("Cargo.lock"), "# lockfile").unwrap();
    std::fs::write(temp_repo.path().join("plexis.db"), "database content").unwrap();
    std::fs::write(
        temp_repo.path().join("untracked.txt"),
        "sensitive local notes",
    )
    .unwrap();

    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let state = AppState::new(store);
    let app = create_router(state.clone());

    // 1. Create Workspace
    let ws_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/workspaces")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "Safety Test Workspace",
                        "canonical_path": temp_repo.path().to_str().unwrap()
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ws_res.status(), StatusCode::CREATED);
    let ws_body = ws_res.into_body().collect().await.unwrap().to_bytes();
    let ws_json: serde_json::Value = serde_json::from_slice(&ws_body).unwrap();
    let ws_id = ws_json["id"].as_str().unwrap();

    // 2. Create Mission using scripted backend for deterministic testing
    let mission_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/missions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "title": "Quickstart Safety Audit",
                        "objective": "Verify no direct commits on main",
                        "workspace_id": ws_id,
                        "backend": "scripted"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mission_res.status(), StatusCode::CREATED);
    let mission_body = mission_res.into_body().collect().await.unwrap().to_bytes();
    let mission_json: serde_json::Value = serde_json::from_slice(&mission_body).unwrap();
    let mission_id = mission_json["id"].as_str().unwrap();

    // 3. Start Mission
    let start_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/missions/{mission_id}/start"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_res.status(), StatusCode::OK);

    // 4. Step mission through execution
    for _ in 0..10 {
        let step_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/missions/{mission_id}/step"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if step_res.status() != StatusCode::OK {
            break;
        }
        let step_body = step_res.into_body().collect().await.unwrap().to_bytes();
        let step_json: serde_json::Value = serde_json::from_slice(&step_body).unwrap();
        let state = step_json["state"].as_str().unwrap();
        if state == "awaiting_acceptance"
            || state == "completed"
            || state == "failed"
            || state == "needs_human"
        {
            break;
        }
    }

    // 5. Invariant Assertion: Check git repository state on main
    let head_out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(temp_repo.path())
        .output()
        .unwrap();
    let current_sha = String::from_utf8_lossy(&head_out.stdout).trim().to_string();

    assert_eq!(
        current_sha, initial_sha,
        "CRITICAL INVARIANT VIOLATION: main branch commit SHA changed before human acceptance! Initial: {}, Current: {}",
        initial_sha, current_sha
    );

    // Verify git log contains only initial commit
    let log_out = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(temp_repo.path())
        .output()
        .unwrap();
    let log_str = String::from_utf8_lossy(&log_out.stdout);
    assert!(
        !log_str.contains("Plexis Agent"),
        "CRITICAL INVARIANT VIOLATION: Agent committed directly to main: {}",
        log_str
    );
    assert!(
        !log_str.contains("Axonel Agent"),
        "CRITICAL INVARIANT VIOLATION: Agent committed directly to main: {}",
        log_str
    );

    // Verify untracked files remain untracked
    let status_out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(temp_repo.path())
        .output()
        .unwrap();
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    assert!(
        status_str.contains("?? Cargo.lock"),
        "Cargo.lock was swept into git index!"
    );
    assert!(
        status_str.contains("?? plexis.db"),
        "plexis.db was swept into git index!"
    );
    assert!(
        status_str.contains("?? untracked.txt"),
        "untracked.txt was swept into git index!"
    );
}
