//! Comprehensive Multi-Agent Lease Fencing and Stale Worker Protection Tests
//!
//! Validates:
//! 1. Monotonic fencing tokens reject stale workers on generation mismatch.
//! 2. Systematic recovery reassignment increments generation and allows clean takeover.
//! 3. External agent backends enforce lease fencing tokens upon completion.
//! 4. Crash-startup reconciler safely clears expired multi-agent leases.

use std::sync::Arc;
use chrono::Duration;

use plexis_core::state::TaskState;
use plexis_core::{
    Agent, Command, CommandTarget, CommandType, ExecutionProfile, Lease, Task, Workflow,
};
use plexis_runtime::backend::FakeAgentBackend;
use plexis_runtime::reconciler::Reconciler;
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::sqlite::SqliteStore;
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, LeaseStore, SessionStore, TaskStore, WorkflowStore,
};

#[tokio::test]
async fn test_multi_agent_lease_fencing_protects_against_stale_workers() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    let wf = Workflow::new("Multi-Agent Fencing", "Testing monotonic lease fencing");
    store.create_workflow(&wf).await.unwrap();

    let mut task = Task::new(wf.id, "Review and audit security patch");
    task.state = TaskState::Ready;
    task.criteria = vec![];
    store.create_task(&task).await.unwrap();

    let reviewer_1 = Agent::new(
        "Reviewer-Initial",
        "Reviewer",
        ExecutionProfile::new("scripted", "default-model"),
    );
    let reviewer_2 = Agent::new(
        "Reviewer-Recovered",
        "Reviewer",
        ExecutionProfile::new("scripted", "default-model"),
    );
    store.create_agent(&reviewer_1).await.unwrap();
    store.create_agent(&reviewer_2).await.unwrap();

    // 1. Initial Reviewer acquires lease: generation = 1
    let lease1 = Lease::new(task.id, reviewer_1.id, Duration::seconds(60));
    let granted1 = store.acquire_lease(&lease1).await.unwrap();
    assert_eq!(granted1.generation, 1, "Initial lease should have generation 1");

    // Command prepared for Reviewer 1 with generation 1
    let cmd1 = Command::new(
        CommandTarget::Agent(reviewer_1.id),
        CommandType::ExecuteTask,
        serde_json::json!({
            "task_id": task.id.to_string(),
            "agent_id": reviewer_1.id.to_string(),
            "workflow_id": wf.id.to_string(),
            "lease_generation": 1,
        }),
        "idempotent-cmd-reviewer-1",
    );

    // 2. Simulate failure/timeout: lease is released, task reset to Ready
    store.release_lease(&granted1.id).await.unwrap();
    task.assigned_agent_id = None;
    task.state = TaskState::Ready;
    store.update_task(&task).await.unwrap();

    // 3. Recovered Reviewer acquires lease: monotonic generation incremented to 2
    let lease2 = Lease::new(task.id, reviewer_2.id, Duration::seconds(60));
    let granted2 = store.acquire_lease(&lease2).await.unwrap();
    assert_eq!(
        granted2.generation, 2,
        "Reassigned lease must monotonically increment generation to 2"
    );

    // Command prepared for Reviewer 2 with generation 2
    let cmd2 = Command::new(
        CommandTarget::Agent(reviewer_2.id),
        CommandType::ExecuteTask,
        serde_json::json!({
            "task_id": task.id.to_string(),
            "agent_id": reviewer_2.id.to_string(),
            "workflow_id": wf.id.to_string(),
            "lease_generation": 2,
        }),
        "idempotent-cmd-reviewer-2",
    );

    // Setup AgentRunner with ScriptedProvider
    let tools = plexis_tools::ToolRegistry::new();
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let provider = Arc::new(plexis_providers::ScriptedProvider::new("scripted"));
    provider.queue_response(plexis_providers::CompletionResponse::text("Review audit completed successfully."));
    let mut runner = AgentRunner::new(store.clone(), tools, verifier);
    runner.register_provider(provider);

    // 4. Stale Reviewer 1 attempts to execute using stale generation 1 -> MUST BE REJECTED
    let stale_result = runner.execute_command(&cmd1).await;
    assert!(
        stale_result.is_err(),
        "Stale worker command must fail lease fencing"
    );
    let err_msg = stale_result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Stale lease generation") || err_msg.contains("is held by agent"),
        "Error message must indicate lease fencing rejection: {}",
        err_msg
    );

    // Verify task was NOT corrupted by stale worker
    let reloaded_task = store.get_task(&task.id).await.unwrap().unwrap();
    assert_ne!(
        reloaded_task.state,
        TaskState::Verified,
        "Stale worker must not have marked task as verified"
    );

    // 5. Valid Recovered Reviewer 2 executes with generation 2 -> MUST SUCCEED
    let valid_result = runner.execute_command(&cmd2).await;
    assert!(
        valid_result.is_ok(),
        "Active worker with valid lease generation 2 must succeed: {:?}",
        valid_result.err()
    );

    let final_task = store.get_task(&task.id).await.unwrap().unwrap();
    assert_eq!(
        final_task.state,
        TaskState::Verified,
        "Recovered worker must complete verification successfully"
    );
    assert_eq!(
        final_task.assigned_agent_id,
        Some(reviewer_2.id),
        "Final task must be attributed to the recovered agent"
    );
}

#[tokio::test]
async fn test_external_backend_lease_fencing_rejection_on_generation_bump() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    let wf = Workflow::new("External Fencing WF", "Test external backend fencing");
    store.create_workflow(&wf).await.unwrap();

    let mut task = Task::new(wf.id, "External agent task");
    task.state = TaskState::Ready;
    task.metadata["backend"] = serde_json::json!("fake_agent");
    task.criteria = vec![];
    store.create_task(&task).await.unwrap();

    let agent1 = Agent::new(
        "FakeAgent-1",
        "Developer",
        ExecutionProfile::new("fake_agent", "default"),
    );
    let agent2 = Agent::new(
        "FakeAgent-2",
        "Developer",
        ExecutionProfile::new("fake_agent", "default"),
    );
    store.create_agent(&agent1).await.unwrap();
    store.create_agent(&agent2).await.unwrap();

    let session1 = plexis_core::Session::new(agent1.id)
        .with_working_directory(std::env::temp_dir().to_string_lossy().to_string());
    let session2 = plexis_core::Session::new(agent2.id)
        .with_working_directory(std::env::temp_dir().to_string_lossy().to_string());
    store.create_session(&session1).await.unwrap();
    store.create_session(&session2).await.unwrap();

    // Acquire lease for agent1 (generation 1)
    let lease1 = Lease::new(task.id, agent1.id, Duration::seconds(60));
    let granted1 = store.acquire_lease(&lease1).await.unwrap();
    assert_eq!(granted1.generation, 1);

    // Stale command for agent1 with generation 1
    let stale_cmd = Command::new(
        CommandTarget::Agent(agent1.id),
        CommandType::ExecuteTask,
        serde_json::json!({
            "task_id": task.id.to_string(),
            "agent_id": agent1.id.to_string(),
            "workflow_id": wf.id.to_string(),
            "lease_generation": 1,
        }),
        "external-stale-cmd",
    );

    // Now release agent1's lease and acquire for agent2 (increments generation to 2)
    store.release_lease(&granted1.id).await.unwrap();
    let lease2 = Lease::new(task.id, agent2.id, Duration::seconds(60));
    let granted2 = store.acquire_lease(&lease2).await.unwrap();
    assert_eq!(granted2.generation, 2);

    let tools = plexis_tools::ToolRegistry::new();
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let runner = AgentRunner::new(store.clone(), tools, verifier)
        .with_backend(Arc::new(FakeAgentBackend::with_default_host()));

    // Stale external execution must fail with Lease error
    let res = runner.execute_command(&stale_cmd).await;
    assert!(res.is_err(), "External runner must reject stale lease generation");
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("Stale lease generation") || err.contains("is held by agent"),
        "Expected lease rejection error, got: {}",
        err
    );
}

#[tokio::test]
async fn test_startup_reconciliation_clears_concurrent_stale_leases() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    let wf = Workflow::new("Concurrent Crash WF", "Test reconciler on multiple tasks");
    store.create_workflow(&wf).await.unwrap();

    // Two parallel tasks: Investigator and Analyst
    let mut task1 = Task::new(wf.id, "Investigator Task");
    task1.state = TaskState::Running;
    let mut task2 = Task::new(wf.id, "Analyst Task");
    task2.state = TaskState::Running;
    store.create_task(&task1).await.unwrap();
    store.create_task(&task2).await.unwrap();

    let agent1 = Agent::new("Investigator", "Investigator", ExecutionProfile::new("mock", "model"));
    let agent2 = Agent::new("Analyst", "Analyst", ExecutionProfile::new("mock", "model"));
    store.create_agent(&agent1).await.unwrap();
    store.create_agent(&agent2).await.unwrap();

    // Expired leases for both tasks (simulating power failure / crash during parallel execution)
    let lease1 = Lease::new(task1.id, agent1.id, Duration::milliseconds(-1000));
    let lease2 = Lease::new(task2.id, agent2.id, Duration::milliseconds(-1000));
    store.acquire_lease(&lease1).await.unwrap();
    store.acquire_lease(&lease2).await.unwrap();

    let reconciler = Reconciler::new(
        store.clone() as Arc<dyn TaskStore>,
        store.clone() as Arc<dyn LeaseStore>,
        store.clone() as Arc<dyn EventStore>,
    )
    .with_workflow_store(store.clone() as Arc<dyn WorkflowStore>)
    .with_command_store(store.clone() as Arc<dyn CommandStore>);

    // Run startup reconciliation
    let report = reconciler.reconcile_startup().await.unwrap();

    assert_eq!(
        report.expired_leases_reclaimed.len(),
        2,
        "Reconciler must reclaim both expired leases"
    );
    assert_eq!(
        report.tasks_unassigned.len(),
        2,
        "Reconciler must unassign both tasks"
    );

    let t1_reloaded = store.get_task(&task1.id).await.unwrap().unwrap();
    let t2_reloaded = store.get_task(&task2.id).await.unwrap().unwrap();
    assert_eq!(t1_reloaded.state, TaskState::Ready);
    assert_eq!(t2_reloaded.state, TaskState::Ready);
    assert!(t1_reloaded.assigned_agent_id.is_none());
    assert!(t2_reloaded.assigned_agent_id.is_none());
}
