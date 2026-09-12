use std::sync::Arc;
use tempfile::tempdir;

use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::{Agent, ExecutionProfile, Session, Workflow};
use plexis_planner::{PlanApplier, PlanProposal, PlanningContext, ProposedTask};
use plexis_providers::{CompletionResponse, ScriptedProvider, ToolCall};
use plexis_runtime::dispatcher::BroadcastCommandDispatcher;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::scheduler::DeterministicScheduler;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::traits::{
    AgentStore, MessageStore, PlanStore, SessionStore, TaskStore, VerificationStore, WorkflowStore,
};
use plexis_storage::SqliteStore;
use plexis_tools::builtin::filesystem::FilesystemTool;
use plexis_tools::ToolRegistry;

#[tokio::test]
async fn test_multi_agent_parallel_and_dependent_workflow() {
    let tmp = tempdir().unwrap();
    let workspace_path = tmp.path().to_path_buf();

    // 1. Initialize SQLite Store
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    // 2. Setup Tools
    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(FilesystemTool));

    // 3. Register Agents with distinct capabilities
    let agent_a = Agent::new(
        "Agent A (Developer)",
        "Code Developer",
        ExecutionProfile::new("scripted", "dev-model"),
    )
    .with_capabilities(vec!["filesystem_write".into()]);

    let agent_b = Agent::new(
        "Agent B (Tester)",
        "Test Engineer",
        ExecutionProfile::new("scripted", "test-model"),
    )
    .with_capabilities(vec!["filesystem_write".into(), "test_runner".into()]);

    let agent_c = Agent::new(
        "Agent C (Integrator)",
        "System Integrator",
        ExecutionProfile::new("scripted", "int-model"),
    )
    .with_capabilities(vec!["filesystem_write".into(), "integration".into()]);

    let agent_a_id = agent_a.id;
    let agent_b_id = agent_b.id;
    let agent_c_id = agent_c.id;

    store.create_agent(&agent_a).await.unwrap();
    store.create_agent(&agent_b).await.unwrap();
    store.create_agent(&agent_c).await.unwrap();

    let sess_a = Session::new(agent_a_id)
        .with_working_directory(workspace_path.to_string_lossy().to_string());
    let sess_b = Session::new(agent_b_id)
        .with_working_directory(workspace_path.to_string_lossy().to_string());
    let sess_c = Session::new(agent_c_id)
        .with_working_directory(workspace_path.to_string_lossy().to_string());
    store.create_session(&sess_a).await.unwrap();
    store.create_session(&sess_b).await.unwrap();
    store.create_session(&sess_c).await.unwrap();

    // 4. Setup Scripted LLM Provider with turns for each agent
    // Agent A writes calc.py
    let a_call = ToolCall::new(
        "call_1",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "calc.py",
            "content": "def add(a, b):\n    return a + b\n"
        })
        .to_string(),
    );
    // Agent B writes test_calc.py and sends message to Agent C
    let b_call_write = ToolCall::new(
        "call_2",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "test_calc.py",
            "content": "import calc\ndef test_add():\n    assert calc.add(2, 3) == 5\n"
        })
        .to_string(),
    );
    let b_call_msg = ToolCall::new(
        "call_3",
        "send_message",
        serde_json::json!({
            "to_agent": agent_c_id.to_string(),
            "message_type": "result",
            "content": "Unit tests written and verified for calc.py"
        })
        .to_string(),
    );
    // Agent C writes README.md
    let c_call = ToolCall::new(
        "call_4",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "README.md",
            "content": "# Math Calculator Feature\nAll tests verified by Agent B.\n"
        })
        .to_string(),
    );

    let provider = Arc::new(ScriptedProvider::new("scripted"));
    // Agent A turns
    provider.queue_response(CompletionResponse::tool_calls(vec![a_call]));
    provider.queue_response(CompletionResponse::text("Created calc.py successfully."));
    // Agent B turns
    provider.queue_response(CompletionResponse::tool_calls(vec![
        b_call_write,
        b_call_msg,
    ]));
    provider.queue_response(CompletionResponse::text(
        "Created test_calc.py and notified Integrator.",
    ));
    // Agent C turns
    provider.queue_response(CompletionResponse::tool_calls(vec![c_call]));
    provider.queue_response(CompletionResponse::text(
        "Integrated and documented feature.",
    ));

    // 5. Setup Runtime: Verifier, Runner, Scheduler
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let mut runner = AgentRunner::new(store.clone(), tool_registry, verifier);
    runner.register_provider(provider);
    let runner = Arc::new(runner);

    let lease_manager = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
    let scheduler = DeterministicScheduler::new(store.clone(), lease_manager, dispatcher, runner);

    // 6. Create Active Workflow
    let mut wf = Workflow::new("Feature WF", "Build math calculator feature");
    wf.state.transition_to(WorkflowState::Active).unwrap();
    store.create_workflow(&wf).await.unwrap();

    // 7. Planner generates 3 tasks: A and B concurrent; C depends on A + B
    let mut proposal = PlanProposal::new(
        "Build math calculator feature",
        "Decompose into impl, test, and docs",
    );
    proposal.tasks.push(ProposedTask {
        temp_id: "task-a".into(),
        objective: "Implement calculator".into(),
        description: Some("Create calc.py".into()),
        criteria: vec!["file:calc.py".into(), "contains:calc.py:def add".into()],
        required_capabilities: vec!["filesystem_write".into()],
        suggested_role: Some("Code Developer".into()),
        priority: 10,
    });
    proposal.tasks.push(ProposedTask {
        temp_id: "task-b".into(),
        objective: "Write unit tests".into(),
        description: Some("Create test_calc.py".into()),
        criteria: vec![
            "file:test_calc.py".into(),
            "contains:test_calc.py:test_add".into(),
        ],
        required_capabilities: vec!["filesystem_write".into(), "test_runner".into()],
        suggested_role: Some("Test Engineer".into()),
        priority: 10,
    });
    proposal.tasks.push(ProposedTask {
        temp_id: "task-c".into(),
        objective: "Integrate and document".into(),
        description: Some("Create README.md".into()),
        criteria: vec![
            "file:README.md".into(),
            "contains:README.md:Math Calculator".into(),
        ],
        required_capabilities: vec!["filesystem_write".into(), "integration".into()],
        suggested_role: Some("System Integrator".into()),
        priority: 5,
    });

    proposal = proposal.with_dependency("task-c", "task-a");
    proposal = proposal.with_dependency("task-c", "task-b");

    let applier = PlanApplier::new(store.clone());
    let ctx = PlanningContext::new(wf.id, "Build math calculator feature");
    let plan_res = applier
        .apply(
            &ctx,
            &proposal,
            "scripted",
            "mock-model",
            50,
            Some(20),
            Some(40),
        )
        .await
        .expect("apply plan");

    assert_eq!(plan_res.created_tasks.len(), 3);
    assert_eq!(plan_res.dependencies_created, 2);

    let task_a_id = *plan_res.created_tasks.get("task-a").unwrap();
    let task_b_id = *plan_res.created_tasks.get("task-b").unwrap();
    let task_c_id = *plan_res.created_tasks.get("task-c").unwrap();

    // Verify initial states: Task A and B are Ready; Task C is Backlog (blocked)
    let t_a = store.get_task(&task_a_id).await.unwrap().unwrap();
    let t_b = store.get_task(&task_b_id).await.unwrap().unwrap();
    let t_c = store.get_task(&task_c_id).await.unwrap().unwrap();
    assert_eq!(t_a.state, TaskState::Ready);
    assert_eq!(t_b.state, TaskState::Ready);
    assert_eq!(t_c.state, TaskState::Backlog);

    // 8. TICK 1: Tasks A and B execute concurrently!
    let tick1_dispatched = scheduler.tick().await.expect("tick 1");
    assert_eq!(
        tick1_dispatched, 2,
        "Both independent tasks A and B should execute in tick 1"
    );

    let t_a_after = store.get_task(&task_a_id).await.unwrap().unwrap();
    let t_b_after = store.get_task(&task_b_id).await.unwrap().unwrap();
    assert_eq!(t_a_after.state, TaskState::Verified);
    assert_eq!(t_b_after.state, TaskState::Verified);

    let verifs_a = store.list_verifications_by_task(&task_a_id).await.unwrap();
    assert_eq!(verifs_a.len(), 1);
    assert_eq!(
        verifs_a[0].verdict,
        plexis_core::VerificationVerdict::Passed
    );

    let verifs_b = store.list_verifications_by_task(&task_b_id).await.unwrap();
    assert_eq!(verifs_b.len(), 1);
    assert_eq!(
        verifs_b[0].verdict,
        plexis_core::VerificationVerdict::Passed
    );

    // Verify Agent B sent message to Agent C
    let msgs_to_c = store.list_messages_for_agent(&agent_c_id).await.unwrap();
    assert_eq!(msgs_to_c.len(), 1);
    assert_eq!(msgs_to_c[0].from_agent, agent_b_id);
    assert!(msgs_to_c[0].content.contains("Unit tests written"));

    // 9. TICK 2: Now that A and B are Verified, Task C becomes runnable!
    let tick2_dispatched = scheduler.tick().await.expect("tick 2");
    assert_eq!(tick2_dispatched, 1, "Task C should execute in tick 2");

    let t_c_after = store.get_task(&task_c_id).await.unwrap().unwrap();
    assert_eq!(t_c_after.state, TaskState::Verified);

    let verifs_c = store.list_verifications_by_task(&task_c_id).await.unwrap();
    assert_eq!(verifs_c.len(), 1);
    assert_eq!(
        verifs_c[0].verdict,
        plexis_core::VerificationVerdict::Passed
    );

    // 10. Verify disk artifacts actually exist and match criteria
    assert!(workspace_path.join("calc.py").exists());
    assert!(workspace_path.join("test_calc.py").exists());
    assert!(workspace_path.join("README.md").exists());

    let readme_content = std::fs::read_to_string(workspace_path.join("README.md")).unwrap();
    assert!(readme_content.contains("Math Calculator"));

    // 11. Verify inspectable records in database
    let plan_rec = store
        .get_plan_record(&plan_res.plan_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(plan_rec.status, plexis_core::PlanStatus::Applied);

    let verifs = store.list_verifications_by_task(&task_c_id).await.unwrap();
    assert_eq!(verifs.len(), 1);
    assert_eq!(verifs[0].verdict, plexis_core::VerificationVerdict::Passed);
}
