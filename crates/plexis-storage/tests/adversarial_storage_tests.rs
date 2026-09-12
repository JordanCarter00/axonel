use chrono::Duration;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, SessionId, TaskId, WorkflowId};
use plexis_core::state::{CommandState, ExecutionState, TaskState};
use plexis_core::{
    Agent, Command, CommandTarget, CommandType, Event, Execution, ExecutionProfile, Lease, Session,
    Task, Workflow,
};
use plexis_storage::error::StorageError;
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, ExecutionStore, LeaseStore, SessionStore, TaskStore,
    WorkflowStore,
};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_adversarial_db_restart_and_full_recovery() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("restart_test.db");
    let path_str = db_path.to_str().expect("valid path");

    let session_id;
    let wf_id;
    let t1_id;
    let t2_id;
    let agent_id;
    let exec_id;
    let lease_id;

    // Phase 1: Initialize database, write interconnected domain objects, drop connection
    {
        let store = SqliteStore::open(path_str).expect("open sqlite on disk");

        // 1. Agent
        let agent = Agent::new(
            "DeployerAgent",
            "DevOps Worker",
            ExecutionProfile::new("anthropic", "claude-3-5-sonnet"),
        );
        agent_id = agent.id;
        store.create_agent(&agent).await.expect("create agent");

        // 2. Session
        let mut session = Session::new(agent_id);
        session.metadata = serde_json::json!({"initiator": "alice", "environment": "production"});
        session = session.with_working_directory("/tmp/workspace");
        session_id = session.id;
        store
            .create_session(&session)
            .await
            .expect("create session");

        // 3. Workflow
        let wf = Workflow::new("Deployment Workflow", "Adversarial test workflow");
        wf_id = wf.id;
        store.create_workflow(&wf).await.expect("create workflow");

        // 4. Tasks & Dependencies
        let mut t1 = Task::new(wf_id, "Build artifacts").with_priority(50);
        let t2 = Task::new(wf_id, "Deploy containers").with_priority(100);
        t1_id = t1.id;
        t2_id = t2.id;
        store.create_task(&t1).await.expect("create t1");
        store.create_task(&t2).await.expect("create t2");
        store
            .add_dependency(&t2_id, &t1_id)
            .await
            .expect("t2 depends on t1");

        // 5. Acquire lease on t1
        t1.transition_to(TaskState::Ready)
            .expect("transition t1 ready");
        t1.transition_to(TaskState::Assigned)
            .expect("transition t1 assigned");
        t1.transition_to(TaskState::Running)
            .expect("transition t1 running");
        store.update_task(&t1).await.expect("update t1");

        let requested_lease = Lease::new(t1_id, agent_id, Duration::seconds(60));
        let granted_lease = store
            .acquire_lease(&requested_lease)
            .await
            .expect("acquire lease");
        assert_eq!(granted_lease.generation, 1);
        lease_id = granted_lease.id;

        // 6. Execution
        let mut exec = Execution::new(t1_id, agent_id, 1)
            .with_session(session_id)
            .with_lease(lease_id);
        exec.mark_running().expect("start execution");
        exec_id = exec.id;
        store
            .create_execution(&exec)
            .await
            .expect("create execution");

        // 7. Command
        let cmd = Command::new(
            CommandTarget::Agent(agent_id),
            CommandType::ExecuteTask,
            serde_json::json!({"action": "deploy"}),
            "cmd-deploy-001",
        );
        store.enqueue_command(&cmd).await.expect("enqueue command");

        // 8. Event
        let event = Event::new(
            "task",
            t1_id.to_string(),
            "task.started",
            serde_json::json!({"executor": agent_id.to_string()}),
        );
        store.append_event(&event).await.expect("append event");

        // Drop store cleanly simulating process termination
    }

    // Phase 2: Re-open the database from disk and reconstruct state from zero
    {
        let store = SqliteStore::open(path_str).expect("reopen sqlite from disk");

        // Verify session
        let recovered_session = store
            .get_session(&session_id)
            .await
            .expect("get session")
            .expect("session exists");
        assert_eq!(recovered_session.agent_id, agent_id);
        assert_eq!(
            recovered_session.working_directory.as_deref(),
            Some("/tmp/workspace")
        );
        assert!(recovered_session.is_active());
        assert_eq!(
            recovered_session.metadata["initiator"],
            serde_json::json!("alice")
        );

        // Verify workflow
        let recovered_wf = store
            .get_workflow(&wf_id)
            .await
            .expect("get workflow")
            .expect("workflow exists");
        assert_eq!(recovered_wf.title, "Deployment Workflow");

        // Verify tasks and graph reconstruction
        let graph = store
            .load_task_graph(&wf_id)
            .await
            .expect("load task graph");
        assert_eq!(graph.task_count(), 2);
        assert!(graph.direct_dependencies(&t2_id).contains(&t1_id));

        let recovered_t1 = store
            .get_task(&t1_id)
            .await
            .expect("get t1")
            .expect("t1 exists");
        assert_eq!(recovered_t1.state, TaskState::Running);

        // Verify execution
        let recovered_exec = store
            .get_execution(&exec_id)
            .await
            .expect("get execution")
            .expect("execution exists");
        assert_eq!(recovered_exec.session_id, Some(session_id));
        assert_eq!(recovered_exec.task_id, t1_id);
        assert_eq!(recovered_exec.agent_id, agent_id);
        assert_eq!(recovered_exec.state, ExecutionState::Running);

        // Verify lease
        let recovered_lease = store
            .get_lease_by_task(&t1_id)
            .await
            .expect("get lease")
            .expect("lease exists");
        assert_eq!(recovered_lease.generation, 1);
        assert_eq!(recovered_lease.agent_id, agent_id);

        // Verify command
        let recovered_cmd = store
            .get_by_idempotency_key("cmd-deploy-001")
            .await
            .expect("find command")
            .expect("command exists");
        assert_eq!(recovered_cmd.state, CommandState::Queued);

        // Verify events
        let events = store
            .list_events_by_aggregate("task", &t1_id.to_string())
            .await
            .expect("get events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "task.started");
    }
}

#[tokio::test]
async fn test_adversarial_duplicate_command_idempotency() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let agent_id = AgentId::new();

    let cmd1 = Command::new(
        CommandTarget::Agent(agent_id),
        CommandType::ExecuteTask,
        serde_json::json!({"action": "run"}),
        "idempotent-key-duplicate-test",
    );

    store.enqueue_command(&cmd1).await.expect("enqueue first");

    // Second command with same idempotency key must fail with IdempotencyConflict
    let cmd2 = Command::new(
        CommandTarget::Agent(agent_id),
        CommandType::CancelExecution,
        serde_json::json!({"action": "cancel"}),
        "idempotent-key-duplicate-test",
    );

    let duplicate_result = store.enqueue_command(&cmd2).await;
    assert!(
        matches!(duplicate_result, Err(StorageError::IdempotencyConflict(_))),
        "Duplicate idempotency key must trigger StorageError::IdempotencyConflict"
    );

    // Existing command must remain intact and undisturbed
    let existing = store
        .get_by_idempotency_key("idempotent-key-duplicate-test")
        .await
        .expect("query command")
        .expect("command found");
    assert_eq!(existing.id, cmd1.id);
    assert_eq!(existing.command_type, CommandType::ExecuteTask);
}

#[tokio::test]
async fn test_adversarial_retrying_command_reclaiming() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");
    let agent_id = AgentId::new();

    let cmd = Command::new(
        CommandTarget::Agent(agent_id),
        CommandType::ExecuteTask,
        serde_json::json!({"action": "run"}),
        "cmd-retry-test-1",
    );
    let cmd_id = cmd.id;
    store.enqueue_command(&cmd).await.expect("enqueue");

    // Claim 1: Queued -> Dispatched
    let claimed1 = store
        .claim_next_queued_command()
        .await
        .expect("claim")
        .expect("command was queued");
    assert_eq!(claimed1.id, cmd_id);
    assert_eq!(claimed1.state, CommandState::Dispatched);

    // No more queued commands
    let empty_claim = store.claim_next_queued_command().await.expect("claim");
    assert!(empty_claim.is_none());

    // Fail command into Retrying state
    let mut to_retry = claimed1;
    to_retry.mark_failed();
    assert_eq!(to_retry.state, CommandState::Retrying);
    store
        .update_command(&to_retry)
        .await
        .expect("update command");

    // Claim 2: Retrying -> Dispatched (verifies retrying commands are reclaimed)
    let claimed2 = store
        .claim_next_queued_command()
        .await
        .expect("claim retrying")
        .expect("command must be reclaimed from retrying state");
    assert_eq!(claimed2.id, cmd_id);
    assert_eq!(claimed2.state, CommandState::Dispatched);
}

#[tokio::test]
async fn test_adversarial_monotonic_lease_generation_on_expiry_and_reacquisition() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let wf = Workflow::new("Lease Gen Test", "Test monotonic fencing token generation");
    store.create_workflow(&wf).await.expect("create wf");

    let t1 = Task::new(wf.id, "Fenced Task");
    let t1_id = t1.id;
    store.create_task(&t1).await.expect("create task");

    let agent1 = Agent::new(
        "Worker 1",
        "Runner",
        ExecutionProfile::new("provider", "model-a"),
    );
    let agent2 = Agent::new(
        "Worker 2",
        "Runner",
        ExecutionProfile::new("provider", "model-b"),
    );
    store.create_agent(&agent1).await.expect("create agent1");
    store.create_agent(&agent2).await.expect("create agent2");

    // Gen 1: First lease acquisition
    let req1 = Lease::new(t1_id, agent1.id, Duration::seconds(10));
    let lease1 = store.acquire_lease(&req1).await.expect("acquire lease 1");
    assert_eq!(lease1.generation, 1);

    // Attempting concurrent lease acquisition fails
    let req_conflict = Lease::new(t1_id, agent2.id, Duration::seconds(10));
    let blocked = store.acquire_lease(&req_conflict).await;
    assert!(matches!(blocked, Err(StorageError::LeaseConflict(_))));

    // Delete expired lease
    store
        .release_lease(&lease1.id)
        .await
        .expect("release lease 1");

    // Gen 2: Second lease acquisition after release/expiry must be monotonic
    let req2 = Lease::new(t1_id, agent2.id, Duration::seconds(10));
    let lease2 = store.acquire_lease(&req2).await.expect("acquire lease 2");
    assert_eq!(
        lease2.generation, 2,
        "Lease generation must monotonically increase to 2"
    );

    // Delete lease again
    store
        .release_lease(&lease2.id)
        .await
        .expect("release lease 2");

    // Gen 3: Third lease acquisition must be 3
    let req3 = Lease::new(t1_id, agent1.id, Duration::seconds(10));
    let lease3 = store.acquire_lease(&req3).await.expect("acquire lease 3");
    assert_eq!(
        lease3.generation, 3,
        "Lease generation must monotonically increase to 3"
    );

    // Verify task table records authoritative generation 3
    let current_gen = store
        .get_current_lease_generation(&t1_id)
        .await
        .expect("get current gen");
    assert_eq!(current_gen, 3);
}

#[tokio::test]
async fn test_adversarial_foreign_key_enforcement() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    // 1. Task referencing non-existent workflow must fail
    let ghost_wf_id = WorkflowId::new();
    let orphan_task = Task::new(ghost_wf_id, "Ghost task");
    let task_err = store.create_task(&orphan_task).await;
    assert!(
        task_err.is_err(),
        "Inserting task with non-existent workflow must violate foreign key constraint"
    );

    // 2. Task dependency referencing non-existent task must fail
    let wf = Workflow::new("FK Test", "Verify FK enforcement");
    store.create_workflow(&wf).await.expect("create wf");
    let valid_task = Task::new(wf.id, "Real task");
    store.create_task(&valid_task).await.expect("create task");

    let ghost_dep_id = TaskId::new();
    let dep_err = store.add_dependency(&valid_task.id, &ghost_dep_id).await;
    assert!(
        dep_err.is_err(),
        "Inserting dependency with non-existent dependency target must violate foreign key"
    );

    // 3. Execution referencing non-existent session must fail
    let ghost_session_id = SessionId::new();
    let orphan_exec =
        Execution::new(valid_task.id, AgentId::new(), 1).with_session(ghost_session_id);
    let exec_err = store.create_execution(&orphan_exec).await;
    assert!(
        exec_err.is_err(),
        "Inserting execution with non-existent session must violate foreign key constraint"
    );
}

#[tokio::test]
async fn test_adversarial_session_and_execution_lifecycle() {
    let store = SqliteStore::open_in_memory().expect("open sqlite in-memory");

    let agent = Agent::new(
        "LeadAgent",
        "Planner",
        ExecutionProfile::new("anthropic", "claude-3-opus"),
    );
    let agent_id = agent.id;
    store.create_agent(&agent).await.expect("create agent");

    let wf = Workflow::new("Session Lifecycle", "Test execution lifecycle");
    store.create_workflow(&wf).await.expect("create wf");
    let task = Task::new(wf.id, "Execute sub-routine");
    let task_id = task.id;
    store.create_task(&task).await.expect("create task");

    // Create session
    let mut session = Session::new(agent_id);
    let session_id = session.id;
    store
        .create_session(&session)
        .await
        .expect("create session");

    // List sessions by agent
    let agent_sessions = store
        .list_sessions_by_agent(&agent_id)
        .await
        .expect("list sessions");
    assert_eq!(agent_sessions.len(), 1);
    assert_eq!(agent_sessions[0].id, session_id);
    assert!(agent_sessions[0].is_active());

    // Create executions
    let mut exec1 = Execution::new(task_id, agent_id, 1).with_session(session_id);
    let mut exec2 = Execution::new(task_id, agent_id, 2).with_session(session_id);
    let id1 = exec1.id;
    let id2 = exec2.id;

    store.create_execution(&exec1).await.expect("create exec1");
    store.create_execution(&exec2).await.expect("create exec2");

    // Transition exec1 to Running, then Completed
    exec1.mark_running().expect("start exec1");
    store.update_execution(&exec1).await.expect("update exec1");

    let running_exec1 = store
        .get_execution(&id1)
        .await
        .expect("get")
        .expect("found");
    assert_eq!(running_exec1.state, ExecutionState::Running);

    exec1.mark_completed().expect("complete exec1");
    store.update_execution(&exec1).await.expect("update exec1");

    let completed_exec1 = store
        .get_execution(&id1)
        .await
        .expect("get")
        .expect("found");
    assert_eq!(completed_exec1.state, ExecutionState::Completed);

    // Fail exec2
    exec2.mark_running().expect("start exec2");
    exec2
        .mark_failed("Compilation syntax error")
        .expect("fail exec2");
    store.update_execution(&exec2).await.expect("update exec2");

    let failed_exec2 = store
        .get_execution(&id2)
        .await
        .expect("get")
        .expect("found");
    assert_eq!(failed_exec2.state, ExecutionState::Failed);
    assert_eq!(
        failed_exec2.error_message.as_deref(),
        Some("Compilation syntax error")
    );

    // Query executions by task
    let task_execs = store
        .list_executions_by_task(&task_id)
        .await
        .expect("list by task");
    assert_eq!(task_execs.len(), 2);

    // Close session
    session.close();
    store
        .update_session(&session)
        .await
        .expect("update session");

    let closed_session = store
        .get_session(&session_id)
        .await
        .expect("get")
        .expect("found");
    assert!(!closed_session.is_active());
    assert!(closed_session.closed_at.is_some());
}
