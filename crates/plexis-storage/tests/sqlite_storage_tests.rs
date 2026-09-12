use chrono::{Duration, Utc};
use plexis_core::ids::TaskId;
use plexis_core::state::{CommandState, TaskState, WorkflowState};
use plexis_core::{
    Agent, Command, CommandTarget, CommandType, Event, ExecutionProfile, Lease, Task, Workflow,
};
use plexis_storage::error::StorageError;
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, LeaseStore, TaskStore, WorkflowStore,
};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_workflow_lifecycle_in_sqlite() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let mut wf = Workflow::new("Code Migration", "Migrate legacy code to modern Rust");
    let wf_id = wf.id;

    // Create
    store.create_workflow(&wf).await.expect("create workflow");

    // Get
    let fetched = store
        .get_workflow(&wf_id)
        .await
        .expect("get workflow")
        .expect("workflow found");
    assert_eq!(fetched.title, "Code Migration");
    assert_eq!(fetched.state, WorkflowState::Draft);

    // Update
    wf.set_state(WorkflowState::Active);
    store.update_workflow(&wf).await.expect("update workflow");

    let updated = store
        .get_workflow(&wf_id)
        .await
        .expect("get workflow")
        .expect("workflow found");
    assert_eq!(updated.state, WorkflowState::Active);

    // List
    let list = store.list_workflows().await.expect("list workflows");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, wf_id);
}

#[tokio::test]
async fn test_task_storage_and_graph_reconstruction() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let wf = Workflow::new("Build Pipeline", "CI/CD Setup");
    store.create_workflow(&wf).await.expect("create workflow");

    let mut t1 = Task::new(wf.id, "Checkout repo").with_priority(10);
    let t2 = Task::new(wf.id, "Run tests").with_priority(20);
    let t3 = Task::new(wf.id, "Publish binaries").with_priority(30);

    let id1 = t1.id;
    let id2 = t2.id;
    let id3 = t3.id;

    store.create_task(&t1).await.expect("create t1");
    store.create_task(&t2).await.expect("create t2");
    store.create_task(&t3).await.expect("create t3");

    // t2 depends on t1; t3 depends on t2
    store.add_dependency(&id2, &id1).await.expect("dep t2->t1");
    store.add_dependency(&id3, &id2).await.expect("dep t3->t2");

    // Verify dependencies
    let deps_t2 = store.get_dependencies(&id2).await.expect("deps of t2");
    assert_eq!(deps_t2, vec![id1]);

    let dependents_t2 = store.get_dependents(&id2).await.expect("dependents of t2");
    assert_eq!(dependents_t2, vec![id3]);

    // Load full TaskGraph from durable state
    let graph = store
        .load_task_graph(&wf.id)
        .await
        .expect("load task graph");
    assert_eq!(graph.task_count(), 3);

    // Verify graph is acyclic and topological sort works
    let order = graph.topological_sort().expect("topological sort");
    assert_eq!(order, vec![id1, id2, id3]);

    // Update task status and persist
    t1.transition_to(TaskState::Ready).unwrap();
    t1.transition_to(TaskState::Assigned).unwrap();
    t1.transition_to(TaskState::Running).unwrap();
    t1.transition_to(TaskState::AwaitingVerification).unwrap();
    t1.transition_to(TaskState::Verified).unwrap();
    store.update_task(&t1).await.expect("update t1");

    let reloaded_t1 = store
        .get_task(&id1)
        .await
        .expect("get t1")
        .expect("t1 found");
    assert_eq!(reloaded_t1.state, TaskState::Verified);
}

#[tokio::test]
async fn test_command_idempotency_and_queue() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let task_id = TaskId::new();
    let cmd = Command::new(
        CommandTarget::Task(task_id),
        CommandType::ExecuteTask,
        serde_json::json!({"action": "start"}),
        "unique-idemp-key-100",
    );

    // Enqueue
    store.enqueue_command(&cmd).await.expect("enqueue command");

    // Duplicate idempotency key must fail with IdempotencyConflict
    let duplicate = Command::new(
        CommandTarget::Task(task_id),
        CommandType::ExecuteTask,
        serde_json::json!({"action": "duplicate"}),
        "unique-idemp-key-100",
    );
    let dup_err = store.enqueue_command(&duplicate).await;
    assert!(matches!(dup_err, Err(StorageError::IdempotencyConflict(_))));

    // Claim next queued command
    let claimed = store
        .claim_next_queued_command()
        .await
        .expect("claim")
        .expect("command claimed");
    assert_eq!(claimed.id, cmd.id);
    assert_eq!(claimed.state, CommandState::Dispatched);
    assert_eq!(claimed.attempts, 1);
    assert!(claimed.dispatched_at.is_some());

    // Nothing queued anymore
    let none_left = store.claim_next_queued_command().await.expect("claim");
    assert!(none_left.is_none());
}

#[tokio::test]
async fn test_lease_mutual_exclusion_and_expiry_reclaim() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let wf = Workflow::new("WF", "Objective");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Task");
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    let agent_1 = Agent::new(
        "Agent 1",
        "Worker",
        ExecutionProfile::new("openai", "gpt-4o"),
    );
    let agent_2 = Agent::new(
        "Agent 2",
        "Worker",
        ExecutionProfile::new("openai", "gpt-4o"),
    );
    store.create_agent(&agent_1).await.unwrap();
    store.create_agent(&agent_2).await.unwrap();

    // Acquire active lease
    let lease_1 = Lease::new(task_id, agent_1.id, Duration::minutes(10));
    store.acquire_lease(&lease_1).await.expect("acquire lease 1");

    // Attempting to acquire a lease on the same task while lease 1 is active fails with LeaseConflict
    let lease_2 = Lease::new(task_id, agent_2.id, Duration::minutes(10));
    let conflict = store.acquire_lease(&lease_2).await;
    assert!(matches!(conflict, Err(StorageError::LeaseConflict(_))));

    // Expired lease reclamation
    let mut expired_lease = Lease::new(task_id, agent_1.id, Duration::seconds(-10));
    expired_lease.expires_at = Utc::now() - Duration::seconds(5);
    // Overwrite with expired lease for testing
    store.release_lease(&lease_1.id).await.unwrap();
    store.acquire_lease(&expired_lease).await.unwrap();

    let reclaimed = store
        .reclaim_expired_leases()
        .await
        .expect("reclaim expired leases");
    assert_eq!(reclaimed, vec![task_id]);

    // After reclamation, task has no lease
    let current = store.get_lease_by_task(&task_id).await.unwrap();
    assert!(current.is_none());
}

#[tokio::test]
async fn test_event_audit_trail_immutability() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let task_id = TaskId::new();
    let evt1 = Event::new(
        "task",
        task_id.to_string(),
        "task.created",
        serde_json::json!({"objective": "test"}),
    );
    let evt2 = Event::new(
        "task",
        task_id.to_string(),
        "task.assigned",
        serde_json::json!({"agent": "agent_1"}),
    );

    store.append_event(&evt1).await.expect("append evt1");
    store.append_event(&evt2).await.expect("append evt2");

    let history = store
        .list_events_by_aggregate("task", &task_id.to_string())
        .await
        .expect("list events");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].event_type, "task.created");
    assert_eq!(history[1].event_type, "task.assigned");
}
