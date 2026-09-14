use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, ExecutionId, TaskId, WorkflowId};
use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::{
    Agent, AgentMessage, Command, CommandTarget, CommandType, ExecutionProfile, MemoryRecord,
    MemoryScope, MessageType, Session, Task, VerificationVerdict, Workflow,
};
use plexis_providers::{
    CapabilityRequirement, CompletionResponse, FailoverRouter, PrivacyPolicy, ProviderDescriptor,
    ProviderHealthTracker, RetryPolicy, ScriptedProvider, ToolCall,
};
use plexis_runtime::dispatcher::BroadcastCommandDispatcher;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::reconciler::Reconciler;
use plexis_runtime::recovery::{RecoveryAction, RecoveryController};
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::scheduler::DeterministicScheduler;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, LeaseStore, MemoryStore, MessageStore, RecoveryStore,
    SessionStore, TaskStore, VerificationStore, WorkflowStore,
};
use plexis_storage::SqliteStore;
use plexis_tools::builtin::{FilesystemTool, GitTool};
use plexis_tools::sandbox::Sandbox;
use plexis_tools::traits::ToolInvocationContext;
use plexis_tools::{Tool, ToolRegistry};

// =========================================================================
// Scenario 1: Restart mid-workflow without duplicate work
// =========================================================================
#[tokio::test]
async fn test_adv_01_restart_mid_workflow() {
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().to_path_buf();

    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let lease_mgr = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));

    let mut wf = Workflow::new("WF-Restart", "Sequential restart test");
    wf.state = WorkflowState::Active;
    store.create_workflow(&wf).await.unwrap();

    let mut t1 = Task::new(wf.id, "Task 1");
    t1.metadata["verification"] = serde_json::json!({
        "file_path": "t1.txt",
        "expected_content": "done1"
    });
    let t1_id = t1.id;
    store.create_task(&t1).await.unwrap();

    let mut t2 = Task::new(wf.id, "Task 2");
    t2.metadata["verification"] = serde_json::json!({
        "file_path": "t2.txt",
        "expected_content": "done2"
    });
    let t2_id = t2.id;
    store.create_task(&t2).await.unwrap();
    store.add_dependency(&t2_id, &t1_id).await.unwrap();

    let agent = Agent::new(
        "Worker",
        "worker",
        ExecutionProfile::new("scripted", "model"),
    )
    .with_capabilities(vec!["filesystem".into()]);
    store.create_agent(&agent).await.unwrap();

    let mut session = Session::new(agent.id);
    session.working_directory = Some(repo_dir.display().to_string());
    store.create_session(&session).await.unwrap();

    let mut tool_reg = ToolRegistry::new();
    tool_reg.register(Arc::new(FilesystemTool));

    let provider = Arc::new(ScriptedProvider::new("scripted"));
    // T1 response
    provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "c1",
        "filesystem",
        serde_json::json!({ "action": "write_file", "path": "t1.txt", "content": "done1" })
            .to_string(),
    )]));
    provider.queue_response(CompletionResponse::text("T1 finished"));

    let mut runner = AgentRunner::new(store.clone(), tool_reg.clone(), verifier.clone());
    runner.register_provider(provider.clone());
    let runner = Arc::new(runner);

    let scheduler =
        DeterministicScheduler::new(store.clone(), lease_mgr.clone(), dispatcher.clone(), runner);

    // Tick 1: T1 runs and finishes
    let dispatched = scheduler.tick().await.unwrap();
    assert_eq!(dispatched, 1);
    assert_eq!(
        store.get_task(&t1_id).await.unwrap().unwrap().state,
        TaskState::Verified
    );

    // CRASH / RESTART: drop scheduler
    drop(scheduler);

    // Run startup reconciler
    let reconciler = Reconciler::new(
        store.clone() as Arc<dyn TaskStore>,
        store.clone() as Arc<dyn LeaseStore>,
        store.clone() as Arc<dyn EventStore>,
    )
    .with_workflow_store(store.clone() as Arc<dyn WorkflowStore>);

    let report = reconciler.reconcile_startup().await.unwrap();
    assert!(report.resumable_workflows.contains(&wf.id));

    // Assert T1 was NOT reset or re-assigned
    assert_eq!(
        store.get_task(&t1_id).await.unwrap().unwrap().state,
        TaskState::Verified
    );

    // Recreate runtime after restart
    // T2 response
    provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "c2",
        "filesystem",
        serde_json::json!({ "action": "write_file", "path": "t2.txt", "content": "done2" })
            .to_string(),
    )]));
    provider.queue_response(CompletionResponse::text("T2 finished"));

    let mut runner2 = AgentRunner::new(store.clone(), tool_reg, verifier.clone());
    runner2.register_provider(provider.clone());
    let runner2 = Arc::new(runner2);

    let scheduler2 = DeterministicScheduler::new(
        store.clone(),
        lease_mgr.clone(),
        dispatcher.clone(),
        runner2,
    );

    // Tick 2: T2 resumes and completes without re-executing T1
    let dispatched2 = scheduler2.tick().await.unwrap();
    assert_eq!(dispatched2, 1);
    assert_eq!(
        store.get_task(&t2_id).await.unwrap().unwrap().state,
        TaskState::Verified
    );
}

// =========================================================================
// Scenario 2: Provider outage with circuit-breaker failover
// =========================================================================
#[tokio::test]
async fn test_adv_02_provider_outage_circuit_breaker_failover() {
    let primary = Arc::new(ScriptedProvider::new("primary"));
    primary.queue_error("503 Service Unavailable: High load");

    let fallback = Arc::new(ScriptedProvider::new("fallback"));
    fallback.queue_response(CompletionResponse::text("Fallback successfully answered"));

    let health_tracker = Arc::new(ProviderHealthTracker::new(1, Duration::from_secs(60)));
    let retry_policy = RetryPolicy::new(0, Duration::from_millis(5), Duration::from_millis(10));

    let router = FailoverRouter::new(
        vec![
            (
                ProviderDescriptor::new("primary", "m1", 1, false, false),
                primary,
            ),
            (
                ProviderDescriptor::new("fallback", "m2", 1, false, false),
                fallback,
            ),
        ],
        health_tracker.clone(),
        retry_policy,
    );

    let req = plexis_providers::CompletionRequest::new(
        "m1",
        vec![plexis_providers::ChatMessage::user("Do task")],
    );
    let res = router
        .execute_with_failover(&req, &PrivacyPolicy::Any, &CapabilityRequirement::default())
        .await
        .expect("failover succeeded");

    assert_eq!(
        res.message.content.as_deref(),
        Some("Fallback successfully answered")
    );
    assert!(
        !health_tracker.is_available("primary"),
        "Primary must be tripped by circuit breaker"
    );
    assert!(
        health_tracker.is_available("fallback"),
        "Fallback must remain healthy"
    );
}

// =========================================================================
// Scenario 3: Partial task completion recovery
// =========================================================================
#[tokio::test]
async fn test_adv_03_partial_task_completion_recovery() {
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().to_path_buf();
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let recovery_ctrl = Arc::new(RecoveryController::new(store.clone(), 3));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));

    let wf = Workflow::new("WF-Partial", "Partial recovery");
    store.create_workflow(&wf).await.unwrap();

    let mut task = Task::new(wf.id, "Multi-step file write");
    task.metadata["verification"] = serde_json::json!({
        "file_path": "final.txt",
        "expected_content": "step1_done and step2_done"
    });
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    let agent = Agent::new(
        "Worker",
        "worker",
        ExecutionProfile::new("scripted", "model"),
    )
    .with_capabilities(vec!["filesystem".into()]);
    store.create_agent(&agent).await.unwrap();

    let mut session = Session::new(agent.id);
    session.working_directory = Some(repo_dir.display().to_string());
    store.create_session(&session).await.unwrap();

    let mut tool_reg = ToolRegistry::new();
    tool_reg.register(Arc::new(FilesystemTool));

    let provider = Arc::new(ScriptedProvider::new("scripted"));
    // Attempt 1: writes step1.txt, but fails to write final.txt
    provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "c1",
        "filesystem",
        serde_json::json!({ "action": "write_file", "path": "step1.txt", "content": "step1_done" })
            .to_string(),
    )]));
    provider.queue_response(CompletionResponse::text("Partial work done"));

    // Attempt 2 (Recovery): reads step1.txt and writes final.txt
    provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "c2",
        "filesystem",
        serde_json::json!({ "action": "read_file", "path": "step1.txt" }).to_string(),
    )]));
    provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "c3",
        "filesystem",
        serde_json::json!({ "action": "write_file", "path": "final.txt", "content": "step1_done and step2_done" }).to_string(),
    )]));
    provider.queue_response(CompletionResponse::text("Completed recovered work"));

    let mut runner =
        AgentRunner::new(store.clone(), tool_reg, verifier).with_recovery_controller(recovery_ctrl);
    runner.register_provider(provider);

    // Attempt 1: fails verification because final.txt is missing
    let cmd1 = Command::new(
        CommandTarget::Agent(agent.id),
        CommandType::ExecuteTask,
        serde_json::json!({ "task_id": task_id.to_string(), "agent_id": agent.id.to_string(), "workflow_id": wf.id.to_string() }),
        "cmd-att-1",
    );
    let _ = runner.execute_command(&cmd1).await;

    let t_after1 = store.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(
        t_after1.state,
        TaskState::Ready,
        "Must be reset to Ready for retry"
    );
    assert!(t_after1.metadata.get("recovery_advice").is_some());
    assert!(
        repo_dir.join("step1.txt").exists(),
        "Partial file must persist"
    );

    // Attempt 2: recovers, reads partial file, writes final file
    let cmd2 = Command::new(
        CommandTarget::Agent(agent.id),
        CommandType::ExecuteTask,
        serde_json::json!({ "task_id": task_id.to_string(), "agent_id": agent.id.to_string(), "workflow_id": wf.id.to_string() }),
        "cmd-att-2",
    );
    let _ = runner.execute_command(&cmd2).await;

    let t_after2 = store.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(
        t_after2.state,
        TaskState::Verified,
        "Task must become Verified after recovery"
    );
}

// =========================================================================
// Scenario 4: Duplicate command invocation idempotency
// =========================================================================
#[tokio::test]
async fn test_adv_04_duplicate_tool_invocation_idempotency() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let agent_id = AgentId::new();

    let cmd = Command::new(
        CommandTarget::Agent(agent_id),
        CommandType::ExecuteTask,
        serde_json::json!({ "op": "sync" }),
        "unique-idempotency-token-123",
    );

    // First enqueue
    store.enqueue_command(&cmd).await.unwrap();

    // Duplicate enqueue with exact same idempotency key must not create a duplicate command
    let duplicate_cmd = Command::new(
        CommandTarget::Agent(agent_id),
        CommandType::ExecuteTask,
        serde_json::json!({ "op": "sync" }),
        "unique-idempotency-token-123",
    );
    let _ = store.enqueue_command(&duplicate_cmd).await;

    // Claim next queued command
    let claimed = store.claim_next_queued_command().await.unwrap();
    assert!(claimed.is_some());
    assert_eq!(
        claimed.unwrap().idempotency_key,
        "unique-idempotency-token-123"
    );

    // No second command in queue
    let second_claim = store.claim_next_queued_command().await.unwrap();
    assert!(
        second_claim.is_none(),
        "Queue must not have duplicated the idempotent command"
    );
}

// =========================================================================
// Scenario 5: Stale agent with expired lease rejected
// =========================================================================
#[tokio::test]
async fn test_adv_05_stale_agent_expired_lease_rejected() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let lease_mgr = LeaseManager::new(store.clone());

    let wf = Workflow::new("WF", "Lease expiry test");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Expiring task");
    store.create_task(&task).await.unwrap();

    let agent = Agent::new("Worker", "worker", ExecutionProfile::new("m", "m"));
    store.create_agent(&agent).await.unwrap();

    // Acquire lease with negative duration (already expired)
    let lease = lease_mgr
        .acquire(task.id, agent.id, chrono::Duration::seconds(-10))
        .await
        .unwrap();

    // Reclaim expired leases
    let reclaimed = store.reclaim_expired_leases().await.unwrap();
    assert!(reclaimed.contains(&task.id));

    // Stale lease token validation fails
    let res = lease_mgr
        .validate_token(&task.id, lease.generation, chrono::Utc::now())
        .await;
    assert!(res.is_err(), "Expired lease must be rejected as invalid");
}

// =========================================================================
// Scenario 6: Stale lease write rejected by fencing token
// =========================================================================
#[tokio::test]
async fn test_adv_06_stale_lease_write_fencing_token() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let lease_mgr = LeaseManager::new(store.clone());

    let wf = Workflow::new("WF", "Fencing token test");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Fenced task");
    store.create_task(&task).await.unwrap();

    let agent1 = Agent::new("A1", "worker", ExecutionProfile::new("m", "m"));
    let agent2 = Agent::new("A2", "worker", ExecutionProfile::new("m", "m"));
    store.create_agent(&agent1).await.unwrap();
    store.create_agent(&agent2).await.unwrap();

    // Lease 1 acquired
    let lease1 = lease_mgr
        .acquire(task.id, agent1.id, chrono::Duration::minutes(1))
        .await
        .unwrap();
    let token1 = lease1.generation;

    // Lease 1 released
    lease_mgr.release(&lease1.id).await.unwrap();

    // Lease 2 acquired (monotonic generation token incremented)
    let lease2 = lease_mgr
        .acquire(task.id, agent2.id, chrono::Duration::minutes(1))
        .await
        .unwrap();
    let token2 = lease2.generation;

    assert!(token2 > token1, "Fencing tokens must be strictly monotonic");

    // Validating token1 fails because generation is stale!
    let res = lease_mgr
        .validate_token(&task.id, token1, chrono::Utc::now())
        .await;
    assert!(res.is_err(), "Stale fencing token must be rejected");

    // Validating token2 succeeds
    let valid_res = lease_mgr
        .validate_token(&task.id, token2, chrono::Utc::now())
        .await;
    assert!(valid_res.is_ok(), "Active fencing token must be valid");
}

// =========================================================================
// Scenario 7: Message ordering preserved across restart
// =========================================================================
#[tokio::test]
async fn test_adv_07_message_ordering_preserved_across_restart() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let mut wf = Workflow::new("WF-Msg", "Message ordering test");
    wf.state = WorkflowState::Active;
    let wf_id = wf.id;
    store.create_workflow(&wf).await.unwrap();

    let from_agent = Agent::new("A1", "sender", ExecutionProfile::new("m", "m"));
    let to_agent = Agent::new("A2", "receiver", ExecutionProfile::new("m", "m"));
    let from_agent_id = from_agent.id;
    let to_agent_id = to_agent.id;
    store.create_agent(&from_agent).await.unwrap();
    store.create_agent(&to_agent).await.unwrap();

    // Send sequence of messages
    for i in 1..=5 {
        let msg = AgentMessage::new(
            from_agent_id,
            to_agent_id,
            wf_id,
            MessageType::Handoff,
            format!("Message {i}"),
        );
        store.send_message(&msg).await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    // Drop and re-query (simulating restart over durable store)
    let messages = store.list_messages_by_workflow(&wf_id).await.unwrap();
    assert_eq!(messages.len(), 5);

    for (i, m) in messages.iter().enumerate() {
        assert_eq!(
            m.content,
            format!("Message {}", i + 1),
            "Message ordering must be strictly preserved"
        );
    }
}

// =========================================================================
// Scenario 8: Memory retrieval isolation across workflows
// =========================================================================
#[tokio::test]
async fn test_adv_08_memory_retrieval_isolation() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    let wf_a = WorkflowId::new();
    let wf_b = WorkflowId::new();

    // Private task memory for Workflow A
    let mut mem_a = MemoryRecord::new(
        MemoryScope::Task,
        "agent_a",
        "Secret proprietary algorithm for Workflow A",
    );
    mem_a.scope_id = Some(wf_a.to_string());
    mem_a.provenance.originating_workflow_id = Some(wf_a);
    store.save_memory(&mem_a).await.unwrap();

    // Private task memory for Workflow B
    let mut mem_b = MemoryRecord::new(
        MemoryScope::Task,
        "agent_b",
        "Public knowledge for Workflow B",
    );
    mem_b.scope_id = Some(wf_b.to_string());
    mem_b.provenance.originating_workflow_id = Some(wf_b);
    store.save_memory(&mem_b).await.unwrap();

    // Query Workflow A's scope
    let a_memories = store
        .list_memories_by_scope(MemoryScope::Task, Some(&wf_a.to_string()))
        .await
        .unwrap();
    assert_eq!(a_memories.len(), 1);
    assert_eq!(
        a_memories[0].content,
        "Secret proprietary algorithm for Workflow A"
    );

    // Query Workflow B's scope
    let b_memories = store
        .list_memories_by_scope(MemoryScope::Task, Some(&wf_b.to_string()))
        .await
        .unwrap();
    assert_eq!(b_memories.len(), 1);
    assert_eq!(b_memories[0].content, "Public knowledge for Workflow B");

    // Workflow B cannot see Workflow A's memory
    assert!(!b_memories.iter().any(|m| m.content.contains("Workflow A")));
}

// =========================================================================
// Scenario 9: Verification rejection followed by recovery
// =========================================================================
#[tokio::test]
async fn test_adv_09_verification_rejection_recovery() {
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().to_path_buf();
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let recovery_ctrl = Arc::new(RecoveryController::new(store.clone(), 3));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));

    let wf = Workflow::new("WF-Rejection", "Rejection recovery");
    store.create_workflow(&wf).await.unwrap();

    let mut task = Task::new(wf.id, "Write code with verification");
    task.metadata["verification"] = serde_json::json!({
        "file_path": "output.rs",
        "expected_content": "fn add(a: i32, b: i32) -> i32 { a + b }"
    });
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    let agent = Agent::new(
        "Coder",
        "developer",
        ExecutionProfile::new("scripted", "model"),
    )
    .with_capabilities(vec!["filesystem".into()]);
    store.create_agent(&agent).await.unwrap();

    let mut session = Session::new(agent.id);
    session.working_directory = Some(repo_dir.display().to_string());
    store.create_session(&session).await.unwrap();

    let mut tool_reg = ToolRegistry::new();
    tool_reg.register(Arc::new(FilesystemTool));

    let provider = Arc::new(ScriptedProvider::new("scripted"));
    // Attempt 1: writes incorrect buggy implementation
    provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "c1",
        "filesystem",
        serde_json::json!({ "action": "write_file", "path": "output.rs", "content": "fn add(a: i32, b: i32) -> i32 { 0 }" }).to_string(),
    )]));
    provider.queue_response(CompletionResponse::text("Buggy implementation"));

    // Attempt 2: writes correct code after recovery advice
    provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "c2",
        "filesystem",
        serde_json::json!({ "action": "write_file", "path": "output.rs", "content": "fn add(a: i32, b: i32) -> i32 { a + b }" }).to_string(),
    )]));
    provider.queue_response(CompletionResponse::text("Fixed implementation"));

    let mut runner =
        AgentRunner::new(store.clone(), tool_reg, verifier).with_recovery_controller(recovery_ctrl);
    runner.register_provider(provider);

    // Attempt 1 fails
    let cmd1 = Command::new(
        CommandTarget::Agent(agent.id),
        CommandType::ExecuteTask,
        serde_json::json!({ "task_id": task_id.to_string(), "agent_id": agent.id.to_string(), "workflow_id": wf.id.to_string() }),
        "cmd-rej-1",
    );
    let _ = runner.execute_command(&cmd1).await;

    let v1 = store.list_verifications_by_task(&task_id).await.unwrap();
    assert_eq!(v1.len(), 1);
    assert_eq!(v1[0].verdict, VerificationVerdict::Failed);

    let t1 = store.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(t1.state, TaskState::Ready);

    // Attempt 2 succeeds
    let cmd2 = Command::new(
        CommandTarget::Agent(agent.id),
        CommandType::ExecuteTask,
        serde_json::json!({ "task_id": task_id.to_string(), "agent_id": agent.id.to_string(), "workflow_id": wf.id.to_string() }),
        "cmd-rej-2",
    );
    let _ = runner.execute_command(&cmd2).await;

    let v2 = store.list_verifications_by_task(&task_id).await.unwrap();
    assert_eq!(v2.len(), 2);
    assert_eq!(v2[1].verdict, VerificationVerdict::Passed);

    let t2 = store.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(t2.state, TaskState::Verified);
}

// =========================================================================
// Scenario 10: Recovery loop prevention with escalation
// =========================================================================
#[tokio::test]
async fn test_adv_10_recovery_loop_prevention_quarantine() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    // Max 3 recovery attempts allowed
    let recovery_ctrl = RecoveryController::new(store.clone(), 3);

    let wf = Workflow::new("WF-Loop", "Recovery loop test");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Repeatedly failing task");
    store.create_task(&task).await.unwrap();

    // Attempt 1 failure -> MutateStrategy
    let (action1, _) = recovery_ctrl
        .diagnose_and_recover(&task, &wf.id, None, 1, "Type mismatch error")
        .await
        .unwrap();
    assert!(matches!(action1, RecoveryAction::MutateStrategy { .. }));

    // Attempt 2 failure with different error -> MutateStrategy
    let (action2, _) = recovery_ctrl
        .diagnose_and_recover(&task, &wf.id, None, 2, "Borrow checker error")
        .await
        .unwrap();
    assert!(matches!(action2, RecoveryAction::MutateStrategy { .. }));

    // Attempt 3 failure -> Exceeds max 3 -> Escalates to Fatal / Escalation
    let (action3, _) = recovery_ctrl
        .diagnose_and_recover(&task, &wf.id, None, 3, "Compilation error")
        .await
        .unwrap();
    assert!(matches!(action3, RecoveryAction::EscalateFatal { .. }));

    // Records created
    let recs = store.list_recovery_records_by_task(&task.id).await.unwrap();
    assert_eq!(recs.len(), 3);
    assert_eq!(recs[2].recovery_action, "escalate_fatal");
    assert_eq!(recs[2].result, plexis_core::RecoveryResult::Escalated);
}

// =========================================================================
// Scenario 11: Git operation failure handled cleanly
// =========================================================================
#[tokio::test]
async fn test_adv_11_git_operation_failure_handled_cleanly() {
    let temp = tempdir().unwrap();
    // Non-git directory
    let non_git_dir = temp.path().to_path_buf();

    let sandbox = Arc::new(Sandbox::new(&non_git_dir));
    let git_tool = GitTool;

    let ctx = ToolInvocationContext::new(
        AgentId::new(),
        ExecutionId::new(),
        TaskId::new(),
        serde_json::json!({
            "action": "status"
        }),
        sandbox,
        non_git_dir,
    );

    // Invoking git in a non-git directory returns a clean ToolOutput with non-zero exit code, never panicking
    let result = git_tool
        .execute(&ctx)
        .await
        .expect("git tool executed cleanly without panic");
    assert_ne!(
        result.exit_code,
        Some(0),
        "Git status in non-git directory must return non-zero exit code"
    );
    assert!(result
        .stderr
        .unwrap_or_default()
        .contains("not a git repository"));
}

// =========================================================================
// Scenario 12: Sandbox escape attempt via tool cleanly rejected
// =========================================================================
#[tokio::test]
async fn test_adv_12_sandbox_escape_path_traversal_rejected() {
    let temp = tempdir().unwrap();
    let workspace = temp.path().to_path_buf();
    let sandbox = Arc::new(Sandbox::new(&workspace));
    let fs_tool = FilesystemTool;

    // Attempt path traversal out of workspace
    let ctx = ToolInvocationContext::new(
        AgentId::new(),
        ExecutionId::new(),
        TaskId::new(),
        serde_json::json!({
            "action": "read_file",
            "path": "../../../../etc/passwd"
        }),
        sandbox,
        workspace,
    );

    let result = fs_tool.execute(&ctx).await;
    assert!(
        result.is_err(),
        "Path traversal out of sandbox must be rejected"
    );
    let err_str = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err_str.contains("denied") || err_str.contains("escapes") || err_str.contains("traversal"),
        "Error message must indicate bounds/authorization violation, got: {}",
        err_str
    );
}
