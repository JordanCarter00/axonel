use std::sync::Arc;
use chrono::{Duration, Utc};
use plexis_core::state::TaskState;
use plexis_core::{Command, CommandTarget, CommandType, Lease, Task, Workflow};
use plexis_runtime::{BroadcastCommandDispatcher, CommandDispatcher, LeaseManager, Reconciler};
use plexis_storage::traits::{AgentStore, EventStore, LeaseStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_broadcast_command_dispatcher() {
    let dispatcher = BroadcastCommandDispatcher::new(16);
    let mut subscriber = dispatcher.subscribe();

    let cmd = Command::new(
        CommandTarget::System,
        CommandType::CreateAgent,
        serde_json::json!({"role": "tester"}),
        "test-key-1",
    );

    dispatcher.dispatch(&cmd).await.expect("dispatch");

    let received = subscriber.recv().await.expect("receive command");
    assert_eq!(received.id, cmd.id);
    assert_eq!(received.idempotency_key, "test-key-1");
}

#[tokio::test]
async fn test_lease_manager_acquire_and_validate() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));

    let wf = Workflow::new("WF", "Objective");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Test Task");
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    let agent = plexis_core::Agent::new(
        "Agent",
        "Worker",
        plexis_core::ExecutionProfile::new("openai", "gpt-4o"),
    );
    let agent_id = agent.id;
    store.create_agent(&agent).await.unwrap();

    let lease_manager = LeaseManager::new(store.clone());

    // Acquire
    let lease = lease_manager
        .acquire(task_id, agent_id, Duration::minutes(5))
        .await
        .expect("acquire lease");
    assert_eq!(lease.task_id, task_id);
    assert_eq!(lease.generation, 1);

    // Validate valid token
    let valid = lease_manager
        .validate_token(&task_id, 1, Utc::now())
        .await;
    assert!(valid.is_ok());

    // Validate stale token
    let invalid = lease_manager
        .validate_token(&task_id, 99, Utc::now())
        .await;
    assert!(invalid.is_err());
}

#[tokio::test]
async fn test_reconciler_reclaims_expired_leases_and_unassigns_tasks() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));

    let wf = Workflow::new("Reconcile WF", "Test reconciliation");
    store.create_workflow(&wf).await.unwrap();

    let agent = plexis_core::Agent::new(
        "Reconcile Agent",
        "Worker",
        plexis_core::ExecutionProfile::new("openai", "gpt-4o"),
    );
    let agent_id = agent.id;
    store.create_agent(&agent).await.unwrap();

    let mut task = Task::new(wf.id, "Stalled Task");
    task.assign_to(agent_id).unwrap();
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    // Insert an expired lease directly
    let mut expired_lease = Lease::new(task_id, agent_id, Duration::seconds(-10));
    expired_lease.expires_at = Utc::now() - Duration::seconds(5);
    store.acquire_lease(&expired_lease).await.unwrap();

    let reconciler = Reconciler::new(store.clone(), store.clone(), store.clone());

    // Run reconciliation
    let report = reconciler.reconcile().await.expect("reconcile");
    assert_eq!(report.expired_leases_reclaimed, vec![task_id]);
    assert_eq!(report.tasks_unassigned, vec![task_id]);

    // Check that task is now Ready and unassigned in storage
    let updated_task = store
        .get_task(&task_id)
        .await
        .unwrap()
        .expect("task exists");
    assert_eq!(updated_task.state, TaskState::Ready);
    assert_eq!(updated_task.assigned_agent_id, None);

    // Check that audit event was appended
    let events = store
        .list_events_by_aggregate("task", &task_id.to_string())
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "task.lease_reclaimed");
}
