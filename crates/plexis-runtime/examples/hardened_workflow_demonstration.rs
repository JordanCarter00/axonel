//! Milestone 4 Hardened Autonomous Runtime Demonstration
//!
//! Demonstrates:
//! 1. Persistent memory indexing and hybrid semantic retrieval
//! 2. Execution sandbox with host process stripping and secret redaction
//! 3. Provider reliability, circuit-breaker health tracking, and failover
//! 4. Context assembly with section prioritization, memories, and omission receipts
//! 5. Failure recovery controller with strategy mutation and loop prevention
//! 6. Crash recovery and state reconciliation across restarts

use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, ExecutionId, TaskId, WorkflowId};
use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::{Agent, ExecutionProfile, MemoryProvenance, MemoryScope, Task, Workflow};
use plexis_memory::{DeterministicEmbeddingModel, MemoryManager, MemoryQuery};
use plexis_providers::{
    CapabilityRequirement, ChatMessage, CompletionRequest, CompletionResponse, FailoverRouter,
    PrivacyPolicy, ProviderDescriptor, ProviderError, ProviderHealthTracker, RetryPolicy,
    ScriptedProvider,
};
use plexis_runtime::context::{ContextBudget, ContextBuilder};
use plexis_runtime::reconciler::Reconciler;
use plexis_runtime::recovery::RecoveryController;
use plexis_storage::traits::{AgentStore, EventStore, LeaseStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;
use plexis_tools::{
    InMemorySecretStore, Sandbox, SecretAcl, ShellTool, Tool, ToolInvocationContext,
};

#[tokio::main]
async fn main() {
    println!("=== Plexis Milestone 4: Hardened Autonomous Runtime Demonstration ===");

    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("plexis_m4_demo.db");
    let store = Arc::new(SqliteStore::open(db_path.to_str().unwrap()).expect("open sqlite"));

    // -------------------------------------------------------------
    // 1. Persistent Memory & Semantic Hybrid Retrieval
    // -------------------------------------------------------------
    println!("\n--- 1. Initializing Persistent Memory Subsystem ---");
    let embedding_model = Arc::new(DeterministicEmbeddingModel::default_128());
    let memory_manager = MemoryManager::new(store.clone(), embedding_model);

    let mem1 = memory_manager
        .create_memory(
            MemoryScope::Project,
            Some("proj_alpha".to_string()),
            "system_spec",
            "Project architecture requires strict SQLite WAL journaling and transactional checkpoints."
                .to_string(),
            0.9,
            MemoryProvenance::default(),
            serde_json::json!({"tags": ["sqlite", "wal", "architecture"]}),
        )
        .await
        .expect("create mem1");

    println!("Created persistent memory record: ID={}", mem1.id);

    let search_results = memory_manager
        .search_memories(
            &MemoryQuery::new()
                .with_text("SQLite WAL architecture journaling checkpoints")
                .with_scopes(vec![MemoryScope::Project]),
        )
        .await
        .expect("search memory");

    assert!(!search_results.is_empty());
    println!(
        "Retrieved memory with hybrid score: {:.2} (Sim: {:.2}, Imp: {:.2})",
        search_results[0].total_score,
        search_results[0].similarity,
        search_results[0].importance_score
    );

    // -------------------------------------------------------------
    // 2. Sandboxing & Secret Isolation
    // -------------------------------------------------------------
    println!("\n--- 2. Sandboxed Tool Execution & Secret Redaction ---");
    let agent_id = AgentId::new();
    let task_id = TaskId::new();
    let exec_id = ExecutionId::new();

    let secret_store = Arc::new(InMemorySecretStore::new());
    let secret_key = "production_api_token_super_secret_99";
    secret_store.set_secret(
        "PROD_API_KEY",
        secret_key,
        SecretAcl::new()
            .with_agent(agent_id)
            .with_task(task_id)
            .with_tool("shell"),
    );

    let working_dir = dir.path().join("workspace");
    std::fs::create_dir_all(&working_dir).unwrap();
    let sandbox = Arc::new(Sandbox::new(&working_dir));
    let shell_tool = ShellTool;

    let tool_ctx = ToolInvocationContext::new(
        agent_id,
        exec_id,
        task_id,
        serde_json::json!({
            "command": "echo \"Key is: $PROD_API_KEY\""
        }),
        sandbox,
        working_dir,
    )
    .with_secret_store(secret_store);

    let tool_output = shell_tool.execute(&tool_ctx).await.expect("execute tool");
    let stdout = tool_output.stdout.unwrap();
    println!("Tool output (automatically redacted): {}", stdout.trim());
    assert!(stdout.contains("[REDACTED]"));
    assert!(!stdout.contains(secret_key));

    // -------------------------------------------------------------
    // 3. Provider Reliability, Health Tracking, and Failover
    // -------------------------------------------------------------
    println!("\n--- 3. Provider Reliability & Circuit-Breaker Failover ---");
    let primary = Arc::new(ScriptedProvider::new("primary_cloud"));
    primary.queue_provider_error(ProviderError::Network("503 Gateway Timeout".to_string()));
    primary.queue_provider_error(ProviderError::Network("Connection Reset".to_string()));

    let fallback = Arc::new(ScriptedProvider::new("secondary_local"));
    fallback.queue_response(CompletionResponse::text(
        "Recovered via local fallback model",
    ));

    let primary_desc = ProviderDescriptor::new("primary_cloud", "gpt-4o", 2, true, false);
    let fallback_desc = ProviderDescriptor::new("secondary_local", "llama3", 2, true, true);

    let health_tracker = Arc::new(ProviderHealthTracker::new(2, Duration::from_secs(30)));
    let retry_policy = RetryPolicy::new(1, Duration::from_millis(5), Duration::from_millis(15));

    let router = FailoverRouter::new(
        vec![(primary_desc, primary), (fallback_desc, fallback)],
        health_tracker.clone(),
        retry_policy,
    );

    let req = CompletionRequest::new("gpt-4o", vec![ChatMessage::user("Verify state invariants")]);
    let completion = router
        .execute_with_failover(
            &req,
            &PrivacyPolicy::Any,
            &CapabilityRequirement::strong_with_tools(),
        )
        .await
        .expect("failover execution");

    println!("Completion response: {:?}", completion.message.content);
    assert_eq!(
        completion.message.content.as_deref(),
        Some("Recovered via local fallback model")
    );

    // -------------------------------------------------------------
    // 4. Context Engine with Provenance and Omission Receipts
    // -------------------------------------------------------------
    println!("\n--- 4. Context Assembly with Provenance & Omission Receipts ---");
    let workflow_id = WorkflowId::new();
    let mut workflow = Workflow::new("Deploy System", "Deploy autonomous system");
    workflow.id = workflow_id;
    workflow.state = WorkflowState::Active;
    store.create_workflow(&workflow).await.expect("create wf");

    let mut agent = Agent::new(
        "LeadEngineer",
        "software_engineer",
        ExecutionProfile::new("openai", "gpt-4o"),
    );
    agent.id = agent_id;
    store.create_agent(&agent).await.expect("create agent");

    let mut task = Task::new(workflow_id, "Deploy verified release");
    task.id = task_id;
    store.create_task(&task).await.expect("create task");

    let budget = ContextBudget {
        max_memories: 1,
        max_artifact_bytes: 50,
        ..Default::default()
    };

    let context_summary = ContextBuilder::new(&agent, "Deploy autonomous system", &task)
        .with_budget(budget)
        .with_system_rule("Invariants must never be bypassed")
        .with_dependency_summary("big_config", "A".repeat(300))
        .with_failure_evidence("Integration test timeout on step 4")
        .with_recovery_advice(
            "tool_adaptation",
            2,
            "Command timed out",
            "Increase process timeout budget",
        )
        .with_memories(search_results)
        .build();

    println!(
        "Assembled context tokens: {}",
        context_summary.estimated_tokens
    );
    println!(
        "Provenance items recorded: {}",
        context_summary.provenance.items.len()
    );
    println!(
        "Omission receipts generated: {}",
        context_summary.omission_receipts.len()
    );
    for receipt in &context_summary.omission_receipts {
        println!(
            "  * Omission Receipt: {} (saved {} bytes/units)",
            receipt.section, receipt.units_saved
        );
    }

    // -------------------------------------------------------------
    // 5. Failure Recovery Controller
    // -------------------------------------------------------------
    println!("\n--- 5. Failure Recovery Controller with Strategy Mutation ---");
    let recovery_controller = RecoveryController::new(store.clone(), 3);
    let (action, record) = recovery_controller
        .diagnose_and_recover(
            &task,
            &workflow_id,
            Some(&exec_id),
            1,
            "Tool execution failed: exit code 127 command not found",
        )
        .await
        .expect("recover");

    println!("Recovery Strategy chosen: {:?}", action);
    println!(
        "Durable Recovery Record: ID={}, Strategy={}, Version={}",
        record.id, record.strategy, record.strategy_version
    );

    // -------------------------------------------------------------
    // 6. Long-Running Workflows & Startup State Reconciliation
    // -------------------------------------------------------------
    println!("\n--- 6. Startup State Reconciliation across Restarts ---");
    let mut orphaned_task = Task::new(workflow_id, "Orphaned worker task");
    orphaned_task.state = TaskState::Running;
    orphaned_task.assigned_agent_id = Some(agent_id);
    store
        .create_task(&orphaned_task)
        .await
        .expect("create task");

    let reconciler = Reconciler::new(
        store.clone() as Arc<dyn TaskStore>,
        store.clone() as Arc<dyn LeaseStore>,
        store.clone() as Arc<dyn EventStore>,
    )
    .with_workflow_store(store.clone() as Arc<dyn WorkflowStore>);

    let report = reconciler
        .reconcile_startup()
        .await
        .expect("startup reconcile");
    println!(
        "Reconciliation Report: {} orphaned tasks reset to Ready, {} resumable workflows",
        report.tasks_unassigned.len(),
        report.resumable_workflows.len()
    );
    assert!(report.tasks_unassigned.contains(&orphaned_task.id));

    println!("\n=== Demonstration Completed Successfully! ===");
}
