use chrono::{Duration, Utc};
use plexis_core::ids::{AgentId, TaskId};
use plexis_core::state::{CommandState, MissionState, TaskState, WorkflowState};
use plexis_core::{
    Agent, AgentMessage, Command, CommandTarget, CommandType, Event, ExecutionProfile, Lease,
    MemoryProvenance, MemoryRecord, MemoryScope, MemoryState, MessageType, Mission,
    MissionBudgetConsumed, MissionCheckpoint, MissionCycle, RecoveryRecord, RecoveryResult, Task,
    Workflow, Workspace,
};
use plexis_storage::error::StorageError;
use plexis_storage::traits::{
    AgentStore, ApprovalStore, CommandStore, EventStore, LeaseStore, MemoryStore, MessageStore,
    MissionStore, PlanStore, RecoveryStore, RetentionStore, TaskStore, WorkflowStore,
    WorkspaceStore,
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
    store
        .acquire_lease(&lease_1)
        .await
        .expect("acquire lease 1");

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

#[tokio::test]
async fn test_messages_persistence_and_query() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let wf = Workflow::new("WF", "Objective");
    store.create_workflow(&wf).await.unwrap();

    let a1 = AgentId::new();
    let a2 = AgentId::new();
    let task = Task::new(wf.id, "Context Task");
    store.create_task(&task).await.unwrap();
    let task_id = task.id;

    let msg = plexis_core::AgentMessage::new(
        a1,
        a2,
        wf.id,
        plexis_core::MessageType::Question,
        "What is the schema?",
    )
    .with_task(task_id)
    .with_payload(serde_json::json!({"field": "status"}));

    store.send_message(&msg).await.expect("send message");

    let loaded = store.get_message(&msg.id).await.unwrap().expect("found");
    assert_eq!(loaded.content, "What is the schema?");
    assert_eq!(loaded.message_type, plexis_core::MessageType::Question);

    let for_agent = store.list_messages_for_agent(&a2).await.unwrap();
    assert_eq!(for_agent.len(), 1);
    assert_eq!(for_agent[0].id, msg.id);

    let for_task = store.list_messages_by_task(&task_id).await.unwrap();
    assert_eq!(for_task.len(), 1);
}

#[tokio::test]
async fn test_approval_and_plan_persistence() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let wf = Workflow::new("WF", "Objective");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Task");
    store.create_task(&task).await.unwrap();

    // Test Approval
    let mut appr = plexis_core::ApprovalRecord::new(task.id, wf.id, "Delete database");
    store.create_approval(&appr).await.unwrap();

    let loaded_appr = store.get_approval(&appr.id).await.unwrap().expect("found");
    assert_eq!(loaded_appr.state, plexis_core::ApprovalState::Pending);

    appr.approve(Some("Approved by admin".into())).unwrap();
    store.update_approval(&appr).await.unwrap();

    let approved = store.get_approval(&appr.id).await.unwrap().expect("found");
    assert_eq!(approved.state, plexis_core::ApprovalState::Approved);
    assert_eq!(approved.reason, Some("Approved by admin".into()));

    // Test Planning Record
    let plan = plexis_core::PlanningRecord::new(
        wf.id,
        "Build feature",
        "openai",
        "gpt-4o",
        serde_json::json!({"tasks": ["task1", "task2"]}),
    )
    .with_validation(true, vec![])
    .mark_applied(serde_json::json!({"created_tasks": 2}))
    .with_telemetry(120, Some(50), Some(100));

    store.save_plan_record(&plan).await.unwrap();

    let loaded_plan = store
        .get_plan_record(&plan.id)
        .await
        .unwrap()
        .expect("found");
    assert_eq!(loaded_plan.status, plexis_core::PlanStatus::Applied);
    assert_eq!(loaded_plan.latency_ms, 120);
    assert_eq!(loaded_plan.prompt_tokens, Some(50));
}

#[tokio::test]
async fn test_decompose_task_transactional() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let wf = Workflow::new("WF", "Objective");
    store.create_workflow(&wf).await.unwrap();

    let parent = Task::new(wf.id, "Parent Task");
    let p_id = parent.id;
    store.create_task(&parent).await.unwrap();

    let downstream = Task::new(wf.id, "Downstream Task");
    let d_id = downstream.id;
    store.create_task(&downstream).await.unwrap();
    store.add_dependency(&d_id, &p_id).await.unwrap();

    // Decompose parent into child_1 -> child_2
    let c1 = Task::new(wf.id, "Child 1").with_parent(p_id);
    let c2 = Task::new(wf.id, "Child 2").with_parent(p_id);
    let c1_id = c1.id;
    let c2_id = c2.id;

    store
        .decompose_task_transactional(&p_id, &[c1, c2], &[(c2_id, c1_id)], &[c2_id])
        .await
        .expect("decompose transactional");

    // Parent is now discarded
    let updated_p = store.get_task(&p_id).await.unwrap().unwrap();
    assert_eq!(updated_p.state, plexis_core::TaskState::Discarded);

    // Children exist
    assert!(store.get_task(&c1_id).await.unwrap().is_some());
    assert!(store.get_task(&c2_id).await.unwrap().is_some());

    // Dependencies: c2 depends on c1, downstream depends on c2 (not p)
    let c2_deps = store.get_dependencies(&c2_id).await.unwrap();
    assert_eq!(c2_deps, vec![c1_id]);

    let d_deps = store.get_dependencies(&d_id).await.unwrap();
    assert_eq!(d_deps, vec![c2_id]);
}

#[tokio::test]
async fn test_memory_store_lifecycle_and_scopes() {
    let store = SqliteStore::open_in_memory().unwrap();

    let mut mem = MemoryRecord::new(
        MemoryScope::Project,
        "compiler_setup",
        "Project requires rustc 1.80+ and nightly feature flags for benchmark tests",
    )
    .with_scope_id("project_plexis")
    .with_importance(0.85)
    .with_embedding(vec![0.1, 0.2, 0.3])
    .with_provenance(MemoryProvenance::new().with_retrieval_reason("build config"));

    let mem_id = mem.id;

    // 1. Save
    store.save_memory(&mem).await.expect("save memory");

    // 2. Get
    let fetched = store.get_memory(&mem_id).await.unwrap().expect("found");
    assert_eq!(fetched.content, mem.content);
    assert_eq!(fetched.scope, MemoryScope::Project);
    assert_eq!(fetched.importance, 0.85);
    assert_eq!(fetched.embedding, Some(vec![0.1, 0.2, 0.3]));
    assert_eq!(fetched.access_count, 0);

    // 3. Update & Mark Accessed
    mem.mark_accessed();
    store.update_memory(&mem).await.expect("update memory");

    let accessed = store.get_memory(&mem_id).await.unwrap().expect("found");
    assert_eq!(accessed.access_count, 1);
    assert!(accessed.accessed_at.is_some());

    // 4. Supersede with new memory
    let new_mem = MemoryRecord::new(
        MemoryScope::Project,
        "compiler_setup_v2",
        "Project now compiles on stable 1.80+ without nightly flags",
    )
    .with_scope_id("project_plexis");
    let new_id = new_mem.id;
    store.save_memory(&new_mem).await.unwrap();

    mem.supersede_with(new_id);
    store.update_memory(&mem).await.unwrap();

    let superseded = store.get_memory(&mem_id).await.unwrap().unwrap();
    assert_eq!(superseded.state, MemoryState::Superseded);
    assert_eq!(superseded.superseded_by, Some(new_id));

    // 5. Query active memories by scope
    let active = store
        .list_active_memories(Some(&[MemoryScope::Project]), Some("project_plexis"), 10)
        .await
        .unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, new_id);

    // 6. Soft-delete old memory and verify it is excluded from list_memories_by_scope
    mem.soft_delete();
    store.update_memory(&mem).await.unwrap();

    let scoped = store
        .list_memories_by_scope(MemoryScope::Project, Some("project_plexis"))
        .await
        .unwrap();
    assert_eq!(scoped.len(), 1);
    assert_eq!(scoped[0].id, new_id);
}

#[tokio::test]
async fn test_recovery_store_lifecycle() {
    let store = SqliteStore::open_in_memory().unwrap();
    let wf = Workflow::new(
        "Recovery WF",
        "Test failure diagnosis and recovery persistence",
    );
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Compile binary");
    store.create_task(&task).await.unwrap();

    let mut rec = RecoveryRecord::new(
        task.id,
        wf.id,
        1,
        "standard_compiler",
        "missing libssl dependency",
        "retry_with_installed_pkg",
    )
    .with_strategy_version(1)
    .with_diagnosis(serde_json::json!({ "missing_lib": "libssl-dev" }))
    .with_action_reason("Install libssl-dev via apt sandbox");

    let rec_id = rec.id;

    // 1. Save
    store
        .save_recovery_record(&rec)
        .await
        .expect("save recovery record");

    // 2. Get
    let fetched = store.get_recovery_record(&rec_id).await.unwrap().unwrap();
    assert_eq!(fetched.attempt, 1);
    assert_eq!(fetched.strategy, "standard_compiler");
    assert_eq!(fetched.result, RecoveryResult::InProgress);

    // 3. Update result
    rec.mark_succeeded();
    store
        .update_recovery_record(&rec)
        .await
        .expect("update recovery record");

    let updated = store.get_recovery_record(&rec_id).await.unwrap().unwrap();
    assert_eq!(updated.result, RecoveryResult::Succeeded);

    // 4. List by task and workflow
    let by_task = store.list_recovery_records_by_task(&task.id).await.unwrap();
    assert_eq!(by_task.len(), 1);
    assert_eq!(by_task[0].id, rec_id);

    let by_wf = store
        .list_recovery_records_by_workflow(&wf.id)
        .await
        .unwrap();
    assert_eq!(by_wf.len(), 1);
}

#[tokio::test]
async fn test_event_store_sequence_cursor_and_live_broadcasting() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut rx = store.subscribe_events();

    // 1. Append 3 events
    let e1 = Event::new(
        "task",
        "t1",
        "task_created",
        serde_json::json!({ "name": "task 1" }),
    );
    let e2 = Event::new(
        "task",
        "t1",
        "task_started",
        serde_json::json!({ "attempt": 1 }),
    );
    let e3 = Event::new(
        "task",
        "t1",
        "task_completed",
        serde_json::json!({ "status": "ok" }),
    );

    store.append_event(&e1).await.unwrap();
    store.append_event(&e2).await.unwrap();
    store.append_event(&e3).await.unwrap();

    // 2. Check live broadcast channel received them with populated sequences
    let recv1 = rx.recv().await.unwrap();
    let recv2 = rx.recv().await.unwrap();
    let recv3 = rx.recv().await.unwrap();

    assert_eq!(recv1.sequence, Some(1));
    assert_eq!(recv2.sequence, Some(2));
    assert_eq!(recv3.sequence, Some(3));
    assert_eq!(recv1.event_type, "task_created");
    assert_eq!(recv3.event_type, "task_completed");

    // 3. Test cursor replay: query after sequence 1
    let replayed = store.list_events_after(1, 10).await.unwrap();
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].sequence, Some(2));
    assert_eq!(replayed[1].sequence, Some(3));

    // 4. Test latest sequence
    let latest_seq = store.get_latest_event_sequence().await.unwrap();
    assert_eq!(latest_seq, 3);
}

#[tokio::test]
async fn test_workspace_store_lifecycle_and_scoped_queries() {
    let store = SqliteStore::open_in_memory().unwrap();

    let ws1 = Workspace::new("plexis-core-project", "/tmp/plexis-core-project");
    let ws2 = Workspace::new("plexis-ui-project", "/tmp/plexis-ui-project");

    store.create_workspace(&ws1).await.unwrap();
    store.create_workspace(&ws2).await.unwrap();

    // 1. Get by ID
    let fetched = store
        .get_workspace(&ws1.id)
        .await
        .unwrap()
        .expect("found ws1");
    assert_eq!(fetched.name, "plexis-core-project");
    assert_eq!(
        fetched.canonical_path,
        std::path::PathBuf::from("/tmp/plexis-core-project")
    );

    // 2. Get by path
    let by_path = store
        .get_workspace_by_path(std::path::Path::new("/tmp/plexis-ui-project"))
        .await
        .unwrap()
        .expect("found ws2 by path");
    assert_eq!(by_path.id, ws2.id);

    // 3. List workspaces
    let list = store.list_workspaces().await.unwrap();
    assert_eq!(list.len(), 2);

    // 4. Workflows and tasks scoped to workspace
    let wf = Workflow::new("Workspace-scoped WF", "Scoped objective").with_workspace_id(ws1.id);
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Scoped Task").with_workspace_id(ws1.id);
    store.create_task(&task).await.unwrap();

    // Query scoped
    let ws1_wfs = store.list_workflows_by_workspace(&ws1.id).await.unwrap();
    assert_eq!(ws1_wfs.len(), 1);
    assert_eq!(ws1_wfs[0].workspace_id, Some(ws1.id));

    let ws2_wfs = store.list_workflows_by_workspace(&ws2.id).await.unwrap();
    assert_eq!(ws2_wfs.len(), 0);

    let ws1_tasks = store.list_tasks_by_workspace(&ws1.id).await.unwrap();
    assert_eq!(ws1_tasks.len(), 1);
    assert_eq!(ws1_tasks[0].workspace_id, Some(ws1.id));

    // 5. Update workspace
    let mut updated_ws = ws1.clone();
    updated_ws.name = "plexis-renamed".to_string();
    store.update_workspace(&updated_ws).await.unwrap();
    let after_update = store.get_workspace(&ws1.id).await.unwrap().unwrap();
    assert_eq!(after_update.name, "plexis-renamed");

    // 6. Delete workspace
    store.delete_workspace(&ws2.id).await.unwrap();
    let remaining = store.list_workspaces().await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, ws1.id);
}

#[tokio::test]
async fn test_retention_policy_pruning() {
    let store = SqliteStore::open_in_memory().unwrap();

    let now = Utc::now();
    let old_time = now - Duration::days(30);

    // Create an old event
    let old_event = Event {
        id: plexis_core::ids::EventId::new(),
        sequence: None,
        aggregate_type: "task".to_string(),
        aggregate_id: "t1".to_string(),
        event_type: "old_event".to_string(),
        payload: serde_json::json!({}),
        actor: None,
        causation_id: None,
        correlation_id: None,
        timestamp: old_time,
    };
    store.append_event(&old_event).await.unwrap();

    // Create a recent event
    let recent_event = Event::new("task", "t1", "recent_event", serde_json::json!({}));
    store.append_event(&recent_event).await.unwrap();

    // Create an old message
    let wf = Workflow::new("Msg WF", "Message pruning");
    store.create_workflow(&wf).await.unwrap();

    let mut old_msg = AgentMessage::new(
        AgentId::new(),
        AgentId::new(),
        wf.id,
        MessageType::Question,
        "old message content",
    );
    old_msg.created_at = old_time;
    store.send_message(&old_msg).await.unwrap();

    // Create recent message
    let recent_msg = AgentMessage::new(
        AgentId::new(),
        AgentId::new(),
        wf.id,
        MessageType::Question,
        "recent message content",
    );
    store.send_message(&recent_msg).await.unwrap();

    // Prune records older than 7 days ago
    let cutoff = now - Duration::days(7);
    let report = store.prune_historical_records(cutoff).await.unwrap();

    assert_eq!(report.pruned_events, 1);
    assert_eq!(report.pruned_messages, 1);

    // Verify recent records are still present
    let events = store.list_recent_events(10).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "recent_event");

    let messages = store.list_messages_by_workflow(&wf.id).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "recent message content");
}

#[tokio::test]
async fn test_mission_store_lifecycle() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    // 1. Create a Mission
    let mut mission = Mission::new(
        "Refactor Configuration",
        "Improve configuration parsing and add validation",
    );
    let mission_id = mission.id;
    store
        .create_mission(&mission)
        .await
        .expect("create mission");

    // 2. Query Mission
    let fetched = store
        .get_mission(&mission_id)
        .await
        .expect("get mission")
        .expect("mission should exist");
    assert_eq!(fetched.id, mission_id);
    assert_eq!(fetched.state, MissionState::Created);
    assert_eq!(fetched.cycle_index, 0);

    // 3. Update Mission State & Cycle
    mission.set_state(MissionState::Running);
    mission.cycle_index = 1;
    mission.budget_consumed.total_executions = 2;
    store
        .update_mission(&mission)
        .await
        .expect("update mission");

    let updated = store
        .get_mission(&mission_id)
        .await
        .expect("get mission")
        .expect("mission exists");
    assert_eq!(updated.state, MissionState::Running);
    assert_eq!(updated.cycle_index, 1);
    assert_eq!(updated.budget_consumed.total_executions, 2);

    // 4. Create Mission Cycle
    let wf = Workflow::new("Cycle 1 WF", "Investigation");
    store.create_workflow(&wf).await.unwrap();
    let cycle = MissionCycle::new(mission_id, 1, wf.id, "investigate");
    store.create_cycle(&cycle).await.expect("create cycle");

    let cycles = store.list_cycles(&mission_id).await.expect("list cycles");
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].phase, "investigate");

    // 5. Create Checkpoint
    let mut task_states = serde_json::Map::new();
    task_states.insert("task-1".to_string(), serde_json::json!("verified"));
    let ckpt = MissionCheckpoint::new(
        mission_id,
        1,
        wf.id,
        serde_json::Value::Object(task_states),
        MissionBudgetConsumed::default(),
    );
    let ckpt_id = ckpt.id;
    store
        .create_checkpoint(&ckpt)
        .await
        .expect("create checkpoint");

    let latest_ckpt = store
        .get_latest_checkpoint(&mission_id)
        .await
        .expect("get latest checkpoint")
        .expect("checkpoint should exist");
    assert_eq!(latest_ckpt.id, ckpt_id);
    assert_eq!(latest_ckpt.cycle_index, 1);

    let ckpts = store
        .list_checkpoints(&mission_id)
        .await
        .expect("list checkpoints");
    assert_eq!(ckpts.len(), 1);

    // 6. List Missions
    let missions = store.list_missions().await.expect("list missions");
    assert_eq!(missions.len(), 1);
    assert_eq!(missions[0].id, mission_id);
}
