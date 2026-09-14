use chrono::Duration;
use plexis_core::state::{CommandState, TaskState};
use plexis_core::{
    Agent, Command, CommandTarget, CommandType, ExecutionProfile, Lease, Task, Workflow,
};
use plexis_runtime::reconciler::Reconciler;
use plexis_storage::sqlite::SqliteStore;
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, LeaseStore, TaskStore, WorkflowStore,
};
use std::sync::Arc;

#[tokio::test]
async fn test_reconciler_resolves_orphaned_dispatched_commands_on_crash() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    // Setup workflow and task
    let wf = Workflow::new("Orphan Workflow", "Test orphaned commands");
    store.create_workflow(&wf).await.unwrap();

    let mut task = Task::new(wf.id, "Orphan Task");
    task.state = TaskState::Running;
    store.create_task(&task).await.unwrap();

    let agent = Agent::new(
        "Worker",
        "Test Agent",
        ExecutionProfile::new("mock", "mock-model"),
    );
    store.create_agent(&agent).await.unwrap();

    // Create an expired lease for the task (simulating worker process crash)
    let lease = Lease::new(task.id, agent.id, Duration::milliseconds(-5000));
    store.acquire_lease(&lease).await.unwrap();

    // Create a command that was Dispatched before the crash
    let mut cmd = Command::new(
        CommandTarget::Agent(agent.id),
        CommandType::ExecuteTask,
        serde_json::json!({
            "task_id": task.id.to_string(),
            "agent_id": agent.id.to_string(),
            "workflow_id": wf.id.to_string(),
            "lease_id": lease.id.to_string(),
            "lease_generation": lease.generation,
        }),
        "idempotent-crash-cmd-1",
    );
    cmd.mark_dispatched();
    store.enqueue_command(&cmd).await.unwrap();
    store.update_command(&cmd).await.unwrap();

    // Verify command is in Dispatched state in store
    let persisted_cmd = store.get_command(&cmd.id).await.unwrap().unwrap();
    assert_eq!(persisted_cmd.state, CommandState::Dispatched);

    // Initialize reconciler with command store
    let reconciler = Reconciler::new(
        store.clone() as Arc<dyn TaskStore>,
        store.clone() as Arc<dyn LeaseStore>,
        store.clone() as Arc<dyn EventStore>,
    )
    .with_workflow_store(store.clone() as Arc<dyn WorkflowStore>)
    .with_command_store(store.clone() as Arc<dyn CommandStore>);

    // Run deep startup reconciliation
    let report = reconciler.reconcile_startup().await.unwrap();

    // Assertions:
    // 1. Lease was reclaimed
    assert_eq!(report.expired_leases_reclaimed.len(), 1);
    assert_eq!(report.expired_leases_reclaimed[0], task.id);

    // 2. Task was reset to Ready
    assert_eq!(report.tasks_unassigned.len(), 1);
    let reloaded_task = store.get_task(&task.id).await.unwrap().unwrap();
    assert_eq!(reloaded_task.state, TaskState::Ready);

    // 3. Orphaned dispatched command was reconciled (marked Failed / resolved)
    assert!(
        report.commands_reconciled.contains(&cmd.id),
        "Orphaned command must be reconciled in report"
    );
    let reloaded_cmd = store.get_command(&cmd.id).await.unwrap().unwrap();
    assert_ne!(
        reloaded_cmd.state,
        CommandState::Dispatched,
        "Command must not remain Dispatched"
    );

    // 4. Audit events were recorded
    let events = store.list_recent_events(20).await.unwrap();
    let orphan_event = events
        .iter()
        .find(|e| e.event_type == "command.orphaned_reconciled");
    assert!(
        orphan_event.is_some(),
        "Must record command.orphaned_reconciled event"
    );
}

#[tokio::test]
async fn test_lease_fencing_token_rejects_stale_generation_and_expired_lease() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    let wf = Workflow::new("Fencing WF", "Test lease fencing");
    store.create_workflow(&wf).await.unwrap();

    let mut task = Task::new(wf.id, "Fenced Task");
    task.state = TaskState::Ready;
    store.create_task(&task).await.unwrap();

    let agent1 = Agent::new(
        "Agent1",
        "Primary",
        ExecutionProfile::new("mock", "mock-model"),
    );
    let agent2 = Agent::new(
        "Agent2",
        "Preemptor",
        ExecutionProfile::new("mock", "mock-model"),
    );
    store.create_agent(&agent1).await.unwrap();
    store.create_agent(&agent2).await.unwrap();

    // Agent1 acquires lease gen 1
    let lease1 = Lease::new(task.id, agent1.id, Duration::seconds(60));
    store.acquire_lease(&lease1).await.unwrap();

    // Stale command created for Agent1 with gen 1
    let stale_cmd = Command::new(
        CommandTarget::Agent(agent1.id),
        CommandType::ExecuteTask,
        serde_json::json!({
            "task_id": task.id.to_string(),
            "agent_id": agent1.id.to_string(),
            "workflow_id": wf.id.to_string(),
            "lease_generation": 1,
        }),
        "stale-gen-cmd",
    );

    // Agent1's lease is released/reclaimed, and Agent2 acquires newer lease with gen 2
    store.release_lease(&lease1.id).await.unwrap();
    let mut lease2 = Lease::new(task.id, agent2.id, Duration::seconds(60));
    lease2.generation = 2;
    store.acquire_lease(&lease2).await.unwrap();

    // Create AgentRunner
    let tools = plexis_tools::ToolRegistry::new();
    let verifier = Arc::new(plexis_runtime::verifier::WorkspaceVerifier::new(
        store.clone(),
    ));
    let runner = plexis_runtime::runner::AgentRunner::new(store.clone(), tools, verifier);

    // Stale command execution must be rejected with Lease error due to stale generation
    let res = runner.execute_command(&stale_cmd).await;
    assert!(res.is_err());
    let err_str = res.unwrap_err().to_string();
    assert!(
        err_str.contains("Stale lease generation") || err_str.contains("is held by agent"),
        "Error message must indicate lease fencing failure: {}",
        err_str
    );
}
