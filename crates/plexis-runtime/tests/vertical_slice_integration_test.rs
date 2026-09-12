use std::sync::Arc;
use tempfile::tempdir;

use plexis_core::state::{ExecutionState, TaskState, WorkflowState};
use plexis_core::{
    Agent, Command, CommandTarget, CommandType, ExecutionProfile, Task, VerificationVerdict,
    Workflow,
};
use plexis_providers::{CompletionResponse, ScriptedProvider, ToolCall};
use plexis_runtime::{
    AgentRunner, BroadcastCommandDispatcher, DeterministicScheduler, LeaseManager,
    WorkspaceVerifier,
};
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, SessionStore, TaskStore, VerificationStore, WorkflowStore,
};
use plexis_storage::SqliteStore;
use plexis_tools::ToolRegistry;

#[tokio::test]
async fn test_end_to_end_first_real_execution_slice() {
    let dir = tempdir().expect("tempdir");
    let workspace_path = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_path).expect("create workspace dir");

    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));

    // 1. Create Workflow
    let mut wf = Workflow::new(
        "File Generation Pipeline",
        "Autonomous workspace file creator",
    );
    wf.state = WorkflowState::Active;
    let wf_id = wf.id;
    store.create_workflow(&wf).await.expect("create workflow");

    // 2. Create Task with verification acceptance criteria
    let mut task = Task::new(wf_id, "Create output.txt with verified greeting")
        .with_description("Write 'Plexis Autonomous Execution Slice' to output.txt");
    task.state = TaskState::Ready;
    task.criteria = vec!["file: output.txt".to_string()];
    task.metadata = serde_json::json!({
        "verification": {
            "file_path": "output.txt",
            "expected_content": "Plexis Autonomous Execution Slice"
        }
    });
    let task_id = task.id;
    store.create_task(&task).await.expect("create task");

    // 3. Create Agent
    let agent = Agent::new(
        "CodeWriterAgent",
        "Software Engineer",
        ExecutionProfile::new("scripted", "test-model"),
    );
    let agent_id = agent.id;
    store.create_agent(&agent).await.expect("create agent");

    // Create session configured with workspace directory
    let mut session = plexis_core::Session::new(agent_id);
    session = session.with_working_directory(workspace_path.to_str().unwrap());
    store
        .create_session(&session)
        .await
        .expect("create session");

    // 4. Setup Provider with scripted agent behavior
    let provider = Arc::new(ScriptedProvider::new("scripted"));

    // Turn 1 response: model invokes filesystem tool to write file
    let tool_call = ToolCall::new(
        "call_fs_1",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "output.txt",
            "content": "Plexis Autonomous Execution Slice"
        })
        .to_string(),
    );
    provider.queue_response(CompletionResponse::tool_calls(vec![tool_call]));

    // Turn 2 response: model reports task complete
    provider.queue_response(CompletionResponse::text(
        "I have created output.txt with the requested greeting.",
    ));

    // 5. Setup Runtime: Tool Registry, Verifier, Runner, Scheduler
    let tools = ToolRegistry::standard_suite();
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let mut runner = AgentRunner::new(store.clone(), tools, verifier);
    runner.register_provider(provider);
    let runner = Arc::new(runner);

    let lease_manager = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(16));
    let scheduler = DeterministicScheduler::new(store.clone(), lease_manager, dispatcher, runner);

    // 6. Execute Scheduler Tick
    let scheduled = scheduler.tick().await.expect("scheduler tick");
    assert_eq!(scheduled, 1, "Scheduler must successfully execute 1 task");

    // 7. Verify Task State transitioned to Verified!
    let updated_task = store
        .get_task(&task_id)
        .await
        .expect("get task")
        .expect("task exists");
    assert_eq!(
        updated_task.state,
        TaskState::Verified,
        "Task must be independently verified and in Verified state"
    );

    // 8. Verify physical file was written to disk and content matches
    let created_file_path = workspace_path.join("output.txt");
    assert!(
        created_file_path.exists(),
        "output.txt must physically exist in workspace"
    );
    let content = std::fs::read_to_string(&created_file_path).expect("read file");
    assert_eq!(content, "Plexis Autonomous Execution Slice");

    // 9. Verify Independent Verification record was persisted
    let verifications = store
        .list_verifications_by_task(&task_id)
        .await
        .expect("list verifications");
    assert_eq!(verifications.len(), 1);
    assert_eq!(verifications[0].verdict, VerificationVerdict::Passed);
    assert_eq!(verifications[0].verifier_kind, "workspace_inspection");
    assert_eq!(verifications[0].evidence["matches"], true);

    // 10. Verify durable audit events trail
    let events = store.list_recent_events(100).await.expect("list events");

    let event_types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();

    assert!(
        event_types.contains(&"execution_started"),
        "Must emit execution_started"
    );
    assert!(
        event_types.contains(&"agent_started"),
        "Must emit agent_started"
    );
    assert!(
        event_types.contains(&"provider_request_started"),
        "Must emit provider_request_started"
    );
    assert!(
        event_types.contains(&"provider_request_completed"),
        "Must emit provider_request_completed"
    );
    assert!(
        event_types.contains(&"tool_started"),
        "Must emit tool_started"
    );
    assert!(
        event_types.contains(&"tool_completed"),
        "Must emit tool_completed"
    );
    assert!(
        event_types.contains(&"message_sent"),
        "Must emit message_sent"
    );
    assert!(
        event_types.contains(&"execution_completed"),
        "Must emit execution_completed"
    );
    assert!(
        event_types.contains(&"verification_started"),
        "Must emit verification_started"
    );
    assert!(
        event_types.contains(&"verification_completed"),
        "Must emit verification_completed"
    );
}

#[tokio::test]
async fn test_failure_provider_unavailable_recorded_durably() {
    let _dir = tempdir().expect("tempdir");
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));

    let agent = Agent::new(
        "FailingAgent",
        "Worker",
        ExecutionProfile::new("scripted", "model"),
    );
    store.create_agent(&agent).await.unwrap();

    let wf = Workflow::new("Fail WF", "Obj");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Task failing on provider");
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    // Provider configured to fail immediately
    let provider = Arc::new(ScriptedProvider::new("scripted"));
    provider.queue_error("503 Service Unavailable: Provider backend down");

    let tools = ToolRegistry::standard_suite();
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let mut runner = AgentRunner::new(store.clone(), tools, verifier);
    runner.register_provider(provider);

    let cmd = Command::new(
        CommandTarget::Agent(agent.id),
        CommandType::ExecuteTask,
        serde_json::json!({
            "task_id": task_id.to_string(),
            "agent_id": agent.id.to_string(),
        }),
        "cmd-fail-provider-test",
    );
    store.enqueue_command(&cmd).await.unwrap();

    let exec = runner.execute_command(&cmd).await.unwrap();
    assert_eq!(exec.state, ExecutionState::Failed);
    assert!(exec
        .error_message
        .unwrap()
        .contains("503 Service Unavailable"));

    // Verify durable execution_failed event
    let events = store.list_recent_events(50).await.unwrap();
    let failed_evt = events.iter().find(|e| e.event_type == "execution_failed");
    assert!(failed_evt.is_some(), "Must produce execution_failed event");

    // Task must be in Failed state
    let reloaded_task = store.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(reloaded_task.state, TaskState::Failed);
}

#[tokio::test]
async fn test_failure_tool_sandbox_policy_violation() {
    let dir = tempdir().expect("tempdir");
    let workspace = dir.path().join("sandbox_root");
    std::fs::create_dir_all(&workspace).unwrap();

    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));

    let agent = Agent::new(
        "AttackerAgent",
        "Tester",
        ExecutionProfile::new("scripted", "model"),
    );
    store.create_agent(&agent).await.unwrap();

    let mut session = plexis_core::Session::new(agent.id);
    session = session.with_working_directory(workspace.to_str().unwrap());
    store.create_session(&session).await.unwrap();

    let wf = Workflow::new("Security WF", "Obj");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Security Escape Test");
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    let provider = Arc::new(ScriptedProvider::new("scripted"));
    // Attempt directory traversal escape
    let malicious_tool_call = ToolCall::new(
        "escape_call",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "../../../etc/shadow",
            "content": "hacked"
        })
        .to_string(),
    );
    provider.queue_response(CompletionResponse::tool_calls(vec![malicious_tool_call]));
    provider.queue_response(CompletionResponse::text("Aborted after error."));

    let tools = ToolRegistry::standard_suite();
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let mut runner = AgentRunner::new(store.clone(), tools, verifier);
    runner.register_provider(provider);

    let cmd = Command::new(
        CommandTarget::Agent(agent.id),
        CommandType::ExecuteTask,
        serde_json::json!({
            "task_id": task_id.to_string(),
            "agent_id": agent.id.to_string(),
        }),
        "cmd-security-escape",
    );
    store.enqueue_command(&cmd).await.unwrap();

    let _ = runner.execute_command(&cmd).await;

    // Verify tool_failed event was emitted due to PermissionDenied
    let events = store.list_recent_events(50).await.unwrap();
    let tool_failed = events.iter().find(|e| e.event_type == "tool_failed");
    assert!(
        tool_failed.is_some(),
        "Must emit tool_failed event on sandbox traversal denial"
    );
    let payload_str = tool_failed.unwrap().payload.to_string();
    assert!(
        payload_str.contains("escapes allowed write roots")
            || payload_str.contains("PermissionDenied")
    );
}

#[tokio::test]
async fn test_failure_independent_verification_detects_missing_evidence() {
    let dir = tempdir().expect("tempdir");
    let workspace = dir.path().join("empty_workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));

    let agent = Agent::new(
        "LazyAgent",
        "Worker",
        ExecutionProfile::new("scripted", "model"),
    );
    store.create_agent(&agent).await.unwrap();

    let mut session = plexis_core::Session::new(agent.id);
    session = session.with_working_directory(workspace.to_str().unwrap());
    store.create_session(&session).await.unwrap();

    let wf = Workflow::new("Verify Fail WF", "Obj");
    store.create_workflow(&wf).await.unwrap();

    // Task requires required_artifact.txt with specific content
    let mut task = Task::new(wf.id, "Create required_artifact.txt");
    task.criteria = vec!["file: required_artifact.txt".to_string()];
    task.metadata = serde_json::json!({
        "verification": {
            "file_path": "required_artifact.txt",
            "expected_content": "VALIDATED"
        }
    });
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    // Agent lies and claims completion without executing tool to create file
    let provider = Arc::new(ScriptedProvider::new("scripted"));
    provider.queue_response(CompletionResponse::text("I finished the file (lie!)"));

    let tools = ToolRegistry::standard_suite();
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let mut runner = AgentRunner::new(store.clone(), tools, verifier);
    runner.register_provider(provider);

    let cmd = Command::new(
        CommandTarget::Agent(agent.id),
        CommandType::ExecuteTask,
        serde_json::json!({
            "task_id": task_id.to_string(),
            "agent_id": agent.id.to_string(),
        }),
        "cmd-lying-agent-test",
    );
    store.enqueue_command(&cmd).await.unwrap();

    let _ = runner.execute_command(&cmd).await;

    // Task MUST be in Failed state because verifier independently rejected it!
    let rejected_task = store.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(
        rejected_task.state,
        TaskState::Failed,
        "Task must be marked Failed when independent verification fails"
    );

    // Durable verification record must state Failed verdict
    let verifications = store.list_verifications_by_task(&task_id).await.unwrap();
    assert_eq!(verifications.len(), 1);
    assert_eq!(verifications[0].verdict, VerificationVerdict::Failed);
    assert!(verifications[0]
        .failure_reason
        .as_ref()
        .unwrap()
        .contains("does not exist"));
}
