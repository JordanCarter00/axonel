use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, ExecutionId};
use plexis_core::protocol::ExecutionRequest;
use plexis_core::state::{MissionState, TaskState};
use plexis_core::task::Task;
use plexis_core::workflow::Workflow;
use plexis_runtime::backend::gemini::GeminiCapabilityProbe;
use plexis_runtime::backend::{AgentBackend, GeminiCliBackend};
use plexis_runtime::mission::engine::MissionEngine;
use plexis_storage::traits::{MissionStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_missing_gemini_backend_fails_fast_with_actionable_diagnostic() {
    let missing_probe = GeminiCapabilityProbe::new()
        .with_custom_path(PathBuf::from("/nonexistent/bin/gemini_cli_missing"))
        .with_home_dir(PathBuf::from("/nonexistent/home"));

    let backend = GeminiCliBackend::new().with_probe(missing_probe);
    assert!(!backend.is_available());

    let dir = tempdir().unwrap();
    let req = ExecutionRequest::new(
        ExecutionId::new(),
        AgentId::new(),
        "Developer",
        "Fix the bug",
        dir.path().to_path_buf(),
    );

    let result = backend.execute(&req, None).await;
    assert!(result.is_err(), "Expected missing binary to return error");
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("not installed") || err_str.contains("not found"),
        "Expected error to clearly state not installed / not found, got: {}",
        err_str
    );
}

#[tokio::test]
async fn test_mission_engine_escalates_immediately_on_fatal_provider_error() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));
    let engine = MissionEngine::new(store.clone());

    // Create mission
    let mission = engine
        .create_mission(
            "Missing Provider Test",
            "Verify immediate escalation",
            None,
            None,
            None,
        )
        .await
        .expect("create mission");

    let mut started = engine.start_mission(mission.id).await.expect("start");

    // Create an active workflow with a task that has an unrecoverable failure
    let wf = Workflow::new("Test Cycle 1", "Verify failure");
    store.create_workflow(&wf).await.expect("create wf");
    started.active_workflow_id = Some(wf.id);
    store
        .update_mission(&started)
        .await
        .expect("update mission");

    let mut task = Task::new(wf.id, "Execute agent pass");
    task.state = TaskState::NeedsHuman;
    task.metadata["fatal_reason"] =
        serde_json::json!("Gemini CLI (`gemini`) is not installed or not found on PATH");
    store.create_task(&task).await.expect("create task");

    // Stepping the mission should immediately escalate to human intervention without looping
    let stepped = engine.step_mission(started.id).await.expect("step");
    assert_eq!(stepped.state, MissionState::NeedsHuman);
    assert_eq!(
        stepped.cycle_index, 0,
        "Should not advance cycles when fatal error occurs"
    );

    let reason = stepped
        .metadata
        .get("escalation_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        reason.contains("Gemini CLI (`gemini`) is not installed or not found on PATH"),
        "Expected escalation reason to contain the actual provider error, got: {}",
        reason
    );
}
