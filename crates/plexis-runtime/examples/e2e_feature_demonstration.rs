//! Realistic End-to-End Demonstration for Plexis Milestone 3.
//!
//! Demonstrates the complete autonomous loop:
//! High-level objective -> LLM planner -> Validated task graph ->
//! Capability-aware agent assignment -> Parallel execution ->
//! Agent-to-agent communication -> Independent verification -> Final verified result.

use std::sync::Arc;
use tempfile::tempdir;

use plexis_core::state::WorkflowState;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("========================================================");
    println!("  PLEXIS — Autonomous Multi-Agent Coordination Demo");
    println!("========================================================\n");

    let tmp = tempdir()?;
    let workspace_path = tmp.path().to_path_buf();
    println!(
        "[1/6] Initialized workspace at: {}",
        workspace_path.display()
    );

    // 1. Storage setup
    let store = Arc::new(SqliteStore::open_in_memory()?);
    println!("[2/6] Authoritative SQLite storage initialized and migrated.");

    // 2. Register Specialized Agents
    let agent_dev = Agent::new(
        "DevBot",
        "Software Engineer",
        ExecutionProfile::new("scripted", "dev-v1"),
    )
    .with_capabilities(vec!["filesystem_write".into()]);

    let agent_qa = Agent::new(
        "QABot",
        "Test Engineer",
        ExecutionProfile::new("scripted", "qa-v1"),
    )
    .with_capabilities(vec!["filesystem_write".into(), "test_runner".into()]);

    let agent_lead = Agent::new(
        "LeadBot",
        "Technical Writer / Integrator",
        ExecutionProfile::new("scripted", "lead-v1"),
    )
    .with_capabilities(vec!["filesystem_write".into(), "integration".into()]);

    let dev_id = agent_dev.id;
    let qa_id = agent_qa.id;
    let lead_id = agent_lead.id;

    store.create_agent(&agent_dev).await?;
    store.create_agent(&agent_qa).await?;
    store.create_agent(&agent_lead).await?;

    let sess_dev =
        Session::new(dev_id).with_working_directory(workspace_path.to_string_lossy().to_string());
    let sess_qa =
        Session::new(qa_id).with_working_directory(workspace_path.to_string_lossy().to_string());
    let sess_lead =
        Session::new(lead_id).with_working_directory(workspace_path.to_string_lossy().to_string());
    store.create_session(&sess_dev).await?;
    store.create_session(&sess_qa).await?;
    store.create_session(&sess_lead).await?;

    println!("[3/6] Registered 3 capability-differentiated agents:");
    println!(
        "      * {} [{:?}]",
        agent_dev.display_name, agent_dev.capabilities
    );
    println!(
        "      * {} [{:?}]",
        agent_qa.display_name, agent_qa.capabilities
    );
    println!(
        "      * {} [{:?}]",
        agent_lead.display_name, agent_lead.capabilities
    );

    // 3. Setup Scripted Provider actions
    let dev_tool_call = ToolCall::new(
        "call_impl",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "calculator.py",
            "content": "def add(x, y):\n    return x + y\n\ndef multiply(x, y):\n    return x * y\n"
        })
        .to_string(),
    );
    let qa_tool_write = ToolCall::new(
        "call_test_write",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "test_calculator.py",
            "content": "from calculator import add, multiply\n\ndef test_math():\n    assert add(2, 3) == 5\n    assert multiply(3, 4) == 12\n"
        }).to_string(),
    );
    let qa_msg_call = ToolCall::new(
        "call_msg_lead",
        "send_message",
        serde_json::json!({
            "to_agent": lead_id.to_string(),
            "message_type": "result",
            "content": "Unit tests written for add and multiply functions; all assertions pass."
        })
        .to_string(),
    );
    let lead_tool_call = ToolCall::new(
        "call_docs",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "README.md",
            "content": "# Math Calculator Component\n\nImplementation in calculator.py, verified by test_calculator.py.\n"
        }).to_string(),
    );

    let provider = Arc::new(ScriptedProvider::new("scripted"));
    provider.queue_response(CompletionResponse::tool_calls(vec![dev_tool_call]));
    provider.queue_response(CompletionResponse::text("Implementation finished."));
    provider.queue_response(CompletionResponse::tool_calls(vec![
        qa_tool_write,
        qa_msg_call,
    ]));
    provider.queue_response(CompletionResponse::text(
        "Tests completed and lead notified.",
    ));
    provider.queue_response(CompletionResponse::tool_calls(vec![lead_tool_call]));
    provider.queue_response(CompletionResponse::text("Documentation finalized."));

    // 4. Runtime setup
    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(FilesystemTool));

    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let mut runner = AgentRunner::new(store.clone(), tool_registry, verifier);
    runner.register_provider(provider);
    let runner = Arc::new(runner);

    let lease_manager = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
    let scheduler = DeterministicScheduler::new(store.clone(), lease_manager, dispatcher, runner);

    // 5. Create Workflow & Planning Request
    let mut wf = Workflow::new(
        "Software Component Feature",
        "Create a small software component consisting of implementation, tests, and documentation",
    );
    wf.state.transition_to(WorkflowState::Active)?;
    store.create_workflow(&wf).await?;

    println!("\n[4/6] High-Level Objective: '{}'", wf.objective);
    println!("      Invoking Planner subsystem...");

    let mut proposal =
        PlanProposal::new(&wf.objective, "Decompose component into 3 dependent tasks");
    proposal.tasks.push(ProposedTask {
        temp_id: "t_impl".into(),
        objective: "Implement math functions".into(),
        description: Some("Create calculator.py".into()),
        criteria: vec![
            "file:calculator.py".into(),
            "contains:calculator.py:def add".into(),
        ],
        required_capabilities: vec!["filesystem_write".into()],
        suggested_role: Some("Software Engineer".into()),
        priority: 10,
    });
    proposal.tasks.push(ProposedTask {
        temp_id: "t_test".into(),
        objective: "Write comprehensive unit tests".into(),
        description: Some("Create test_calculator.py".into()),
        criteria: vec![
            "file:test_calculator.py".into(),
            "contains:test_calculator.py:assert add".into(),
        ],
        required_capabilities: vec!["filesystem_write".into(), "test_runner".into()],
        suggested_role: Some("Test Engineer".into()),
        priority: 10,
    });
    proposal.tasks.push(ProposedTask {
        temp_id: "t_docs".into(),
        objective: "Document and finalize component".into(),
        description: Some("Create README.md".into()),
        criteria: vec![
            "file:README.md".into(),
            "contains:README.md:Math Calculator".into(),
        ],
        required_capabilities: vec!["filesystem_write".into(), "integration".into()],
        suggested_role: Some("Technical Writer".into()),
        priority: 5,
    });

    proposal = proposal.with_dependency("t_docs", "t_impl");
    proposal = proposal.with_dependency("t_docs", "t_test");

    let applier = PlanApplier::new(store.clone());
    let ctx = PlanningContext::new(wf.id, &wf.objective);
    let plan_result = applier
        .apply(
            &ctx,
            &proposal,
            "scripted",
            "planner-v1",
            35,
            Some(25),
            Some(50),
        )
        .await?;

    println!(
        "      Plan validated & applied! Created {} tasks with {} dependencies.",
        plan_result.created_tasks.len(),
        plan_result.dependencies_created
    );

    // 6. Execute Schedule Cycles
    println!("\n[5/6] Executing Scheduler Control Loop...");

    // Tick 1: Implementation and Testing execute concurrently
    println!("      Tick 1: Dispatching independent tasks in parallel...");
    let tick1_count = scheduler.tick().await?;
    println!(
        "      Tick 1 completed: {} tasks executed concurrently.",
        tick1_count
    );

    // Verify Tick 1 tasks
    let t_impl_id = plan_result.created_tasks.get("t_impl").unwrap();
    let t_test_id = plan_result.created_tasks.get("t_test").unwrap();
    let _t_impl = store.get_task(t_impl_id).await?.unwrap();
    let _t_test = store.get_task(t_test_id).await?.unwrap();

    let verif_impl = store.list_verifications_by_task(t_impl_id).await?;
    let verif_test = store.list_verifications_by_task(t_test_id).await?;
    println!(
        "      Independent Verification: calculator.py -> {:?}",
        verif_impl[0].verdict
    );
    println!(
        "      Independent Verification: test_calculator.py -> {:?}",
        verif_test[0].verdict
    );

    // Check message sent from QA to Lead
    let lead_msgs = store.list_messages_for_agent(&lead_id).await?;
    println!(
        "      Agent-to-Agent Message: {} message(s) delivered to LeadBot.",
        lead_msgs.len()
    );
    if let Some(first_msg) = lead_msgs.first() {
        println!("        > \"{}\"", first_msg.content);
    }

    // Tick 2: Documentation task executes now that dependencies are verified
    println!("\n      Tick 2: Dispatching downstream dependent task...");
    let tick2_count = scheduler.tick().await?;
    println!("      Tick 2 completed: {} task executed.", tick2_count);

    let t_docs_id = plan_result.created_tasks.get("t_docs").unwrap();
    let _t_docs = store.get_task(t_docs_id).await?.unwrap();
    let verif_docs = store.list_verifications_by_task(t_docs_id).await?;
    println!(
        "      Independent Verification: README.md -> {:?}",
        verif_docs[0].verdict
    );

    // 7. Results summary
    println!("\n[6/6] Workflow Successfully Completed and Verified!");
    println!("========================================================");
    println!("Summary of Generated Artifacts:");
    println!(
        "  - calculator.py: ({} bytes)",
        workspace_path.join("calculator.py").metadata()?.len()
    );
    println!(
        "  - test_calculator.py: ({} bytes)",
        workspace_path.join("test_calculator.py").metadata()?.len()
    );
    println!(
        "  - README.md: ({} bytes)",
        workspace_path.join("README.md").metadata()?.len()
    );

    let plan_rec = store.get_plan_record(&plan_result.plan_id).await?.unwrap();
    println!("\nPersistent Plan Record (id: {}):", plan_rec.id);
    println!("  - Status: {:?}", plan_rec.status);
    println!(
        "  - Provider/Model: {} / {}",
        plan_rec.provider, plan_rec.model
    );
    println!("  - Latency: {} ms", plan_rec.latency_ms);
    println!("========================================================\n");

    Ok(())
}
