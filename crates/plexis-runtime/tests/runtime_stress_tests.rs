use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempdir;

use async_trait::async_trait;
use plexis_core::ids::AgentId;
use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::{Agent, ExecutionProfile, Session, Task, Workflow};
use plexis_providers::{
    ChatRole, CompletionRequest, CompletionResponse, Provider, ProviderError, ToolCall,
};
use plexis_runtime::dispatcher::BroadcastCommandDispatcher;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::recovery::RecoveryController;
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::scheduler::DeterministicScheduler;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::traits::{
    AgentStore, EventStore, RecoveryStore, SessionStore, TaskStore, WorkflowStore,
};
use plexis_storage::SqliteStore;
use plexis_tools::builtin::FilesystemTool;
use plexis_tools::ToolRegistry;

/// Deterministic provider for stress testing capable of serving parallel agent execution.
struct StressTestProvider {
    pub qa_agent_id: AgentId,
    pub provider_requests: AtomicUsize,
    pub tool_calls: AtomicUsize,
}

impl StressTestProvider {
    pub fn new(qa_agent_id: AgentId) -> Self {
        Self {
            qa_agent_id,
            provider_requests: AtomicUsize::new(0),
            tool_calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Provider for StressTestProvider {
    fn id(&self) -> &str {
        "stress_provider"
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        self.provider_requests.fetch_add(1, Ordering::SeqCst);

        // If the last message is a Tool response, the agent has executed the tool(s).
        // Return a final text response to complete the task turn cleanly.
        if let Some(last_msg) = request.messages.last() {
            if last_msg.role == ChatRole::Tool {
                return Ok(CompletionResponse::text("Task execution complete."));
            }
        }

        // Combine all message text to inspect task objective and recovery advice
        let all_text: String = request
            .messages
            .iter()
            .filter_map(|m| m.content.as_deref())
            .collect::<Vec<_>>()
            .join("\n");

        let mut tool_calls = Vec::new();

        if all_text.contains("Task 1 (Init)") {
            self.tool_calls.fetch_add(2, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c1_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "env.txt",
                    "content": "initialized"
                })
                .to_string(),
            ));
            tool_calls.push(ToolCall::new(
                "c1_mem",
                "save_memory",
                serde_json::json!({
                    "content": "System environment initialized",
                    "scope": "project"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 2 (Dev Feature A)") {
            self.tool_calls.fetch_add(2, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c2_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "feature_a.rs",
                    "content": "pub fn feature_a() {}"
                })
                .to_string(),
            ));
            tool_calls.push(ToolCall::new(
                "c2_msg",
                "send_message",
                serde_json::json!({
                    "to_agent": self.qa_agent_id.to_string(),
                    "message_type": "status_update",
                    "content": "Feature A ready for testing"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 3 (Dev Feature B)") {
            self.tool_calls.fetch_add(2, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c3_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "feature_b.rs",
                    "content": "pub fn feature_b() {}"
                })
                .to_string(),
            ));
            tool_calls.push(ToolCall::new(
                "c3_mem",
                "save_memory",
                serde_json::json!({
                    "content": "Feature B relies on async worker queue",
                    "scope": "project"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 4 (QA Plan)") {
            self.tool_calls.fetch_add(2, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c4_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "qa_plan.md",
                    "content": "QA Plan approved"
                })
                .to_string(),
            ));
            tool_calls.push(ToolCall::new(
                "c4_mem",
                "search_memory",
                serde_json::json!({
                    "query": "environment"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 5 (Ops Setup)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c5_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "ops_env.json",
                    "content": "{\"ops\": \"ready\"}"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 6 (Dev Test A)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c6_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "test_a.rs",
                    "content": "#[test] fn test_a() {}"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 7 (Dev Test B)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c7_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "test_b.rs",
                    "content": "#[test] fn test_b() {}"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 8 (QA Execution)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c8_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "qa_results.json",
                    "content": "{\"status\": \"passed\"}"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 9 (Ops Config)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c9_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "deploy_cfg.toml",
                    "content": "env = \"staging\""
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 10 (Staging Bottleneck)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            // Deliberate failure injection on attempt 1:
            // Check if prompt contains recovery strategy or previous failure evidence
            if all_text.contains("PREVIOUS ATTEMPT FAILED")
                || all_text.contains("RECOVERY STRATEGY")
                || all_text.contains("prompt_refinement")
                || all_text.contains("Deconstruct acceptance criteria")
            {
                // Corrected execution on retry
                tool_calls.push(ToolCall::new(
                    "c10_fs_fixed",
                    "filesystem",
                    serde_json::json!({
                        "action": "write_file",
                        "path": "staging.log",
                        "content": "staging_verified"
                    })
                    .to_string(),
                ));
            } else {
                // Deliberate mismatch on attempt 1 to trigger failure and RecoveryController
                tool_calls.push(ToolCall::new(
                    "c10_fs_fail",
                    "filesystem",
                    serde_json::json!({
                        "action": "write_file",
                        "path": "staging.log",
                        "content": "staging_incomplete"
                    })
                    .to_string(),
                ));
            }
        } else if all_text.contains("Task 11 (Dev Docs)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c11_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "README.md",
                    "content": "# Stress Test Project"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 12 (Ops Package)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c12_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "package.tar",
                    "content": "mock package content"
                })
                .to_string(),
            ));
        } else if all_text.contains("Task 13 (QA Final Audit)") {
            self.tool_calls.fetch_add(1, Ordering::SeqCst);
            tool_calls.push(ToolCall::new(
                "c13_fs",
                "filesystem",
                serde_json::json!({
                    "action": "write_file",
                    "path": "AUDIT.md",
                    "content": "Audit Approved"
                })
                .to_string(),
            ));
        } else {
            return Ok(CompletionResponse::text("Unknown task completed."));
        }

        Ok(CompletionResponse::tool_calls(tool_calls))
    }
}

#[tokio::test]
async fn test_runtime_stress_dag_execution_and_metrics() {
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().to_path_buf();

    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let lease_mgr = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let recovery_controller = Arc::new(RecoveryController::new(store.clone(), 3));

    // 1. Create 3 agents (Dev, QA, Ops)
    let dev_agent = Agent::new(
        "DevAgent",
        "Developer",
        ExecutionProfile::new("stress_provider", "stress_model"),
    )
    .with_capabilities(vec!["dev".into(), "filesystem".into()]);
    store.create_agent(&dev_agent).await.unwrap();

    let qa_agent = Agent::new(
        "QAAgent",
        "QA Specialist",
        ExecutionProfile::new("stress_provider", "stress_model"),
    )
    .with_capabilities(vec!["qa".into(), "filesystem".into()]);
    store.create_agent(&qa_agent).await.unwrap();

    let ops_agent = Agent::new(
        "OpsAgent",
        "Ops Engineer",
        ExecutionProfile::new("stress_provider", "stress_model"),
    )
    .with_capabilities(vec!["ops".into(), "filesystem".into()]);
    store.create_agent(&ops_agent).await.unwrap();

    // Create persistent sessions for each agent
    for agent in [&dev_agent, &qa_agent, &ops_agent] {
        let mut session = Session::new(agent.id);
        session.working_directory = Some(repo_dir.display().to_string());
        store.create_session(&session).await.unwrap();
    }

    // 2. Setup Tool Registry and Provider
    let mut tool_reg = ToolRegistry::new();
    tool_reg.register(Arc::new(FilesystemTool));

    let stress_provider = Arc::new(StressTestProvider::new(qa_agent.id));

    let mut runner = AgentRunner::new(store.clone(), tool_reg, verifier)
        .with_recovery_controller(recovery_controller);
    runner.register_provider(stress_provider.clone());
    let runner = Arc::new(runner);

    let scheduler = DeterministicScheduler::new(
        store.clone(),
        lease_mgr.clone(),
        dispatcher.clone(),
        runner.clone(),
    );

    // 3. Create Workflow and 13 Tasks across 6 dependency tiers
    let mut wf = Workflow::new("WF-Stress-13", "Multi-tier DAG execution stress test");
    wf.state = WorkflowState::Active;
    store.create_workflow(&wf).await.unwrap();

    // Helper to create and store task
    let make_task = |objective: &str, role: &str, cap: &str, file: &str, expected: &str| {
        let mut t = Task::new(wf.id, objective);
        t.metadata["suggested_role"] = serde_json::json!(role);
        t.metadata["required_capabilities"] = serde_json::json!([cap, "filesystem"]);
        t.metadata["verification"] = serde_json::json!({
            "file_path": file,
            "expected_content": expected,
        });
        t
    };

    // Tier 1 (Root)
    let t1 = make_task("Task 1 (Init)", "ops", "ops", "env.txt", "initialized");
    store.create_task(&t1).await.unwrap();

    // Tier 2 (Parallel Fan-out from T1)
    let t2 = make_task(
        "Task 2 (Dev Feature A)",
        "dev",
        "dev",
        "feature_a.rs",
        "feature_a",
    );
    let t3 = make_task(
        "Task 3 (Dev Feature B)",
        "dev",
        "dev",
        "feature_b.rs",
        "feature_b",
    );
    let t4 = make_task("Task 4 (QA Plan)", "qa", "qa", "qa_plan.md", "approved");
    let t5 = make_task("Task 5 (Ops Setup)", "ops", "ops", "ops_env.json", "ready");
    for t in [&t2, &t3, &t4, &t5] {
        store.create_task(t).await.unwrap();
        store.add_dependency(&t.id, &t1.id).await.unwrap();
    }

    // Tier 3 (Parallel Fan-out)
    let t6 = make_task("Task 6 (Dev Test A)", "dev", "dev", "test_a.rs", "test_a");
    let t7 = make_task("Task 7 (Dev Test B)", "dev", "dev", "test_b.rs", "test_b");
    let t8 = make_task(
        "Task 8 (QA Execution)",
        "qa",
        "qa",
        "qa_results.json",
        "passed",
    );
    let t9 = make_task(
        "Task 9 (Ops Config)",
        "ops",
        "ops",
        "deploy_cfg.toml",
        "staging",
    );
    store.create_task(&t6).await.unwrap();
    store.add_dependency(&t6.id, &t2.id).await.unwrap();

    store.create_task(&t7).await.unwrap();
    store.add_dependency(&t7.id, &t3.id).await.unwrap();

    store.create_task(&t8).await.unwrap();
    store.add_dependency(&t8.id, &t4.id).await.unwrap();

    store.create_task(&t9).await.unwrap();
    store.add_dependency(&t9.id, &t5.id).await.unwrap();

    // Tier 4 (Bottleneck aggregation of T6, T7, T8, T9)
    let t10 = make_task(
        "Task 10 (Staging Bottleneck)",
        "qa",
        "qa",
        "staging.log",
        "staging_verified",
    );
    store.create_task(&t10).await.unwrap();
    for dep_id in [&t6.id, &t7.id, &t8.id, &t9.id] {
        store.add_dependency(&t10.id, dep_id).await.unwrap();
    }

    // Tier 5 (Parallel Fan-out from T10)
    let t11 = make_task(
        "Task 11 (Dev Docs)",
        "dev",
        "dev",
        "README.md",
        "Stress Test Project",
    );
    let t12 = make_task(
        "Task 12 (Ops Package)",
        "ops",
        "ops",
        "package.tar",
        "mock package",
    );
    for t in [&t11, &t12] {
        store.create_task(t).await.unwrap();
        store.add_dependency(&t.id, &t10.id).await.unwrap();
    }

    // Tier 6 (Terminal Aggregation)
    let t13 = make_task(
        "Task 13 (QA Final Audit)",
        "qa",
        "qa",
        "AUDIT.md",
        "Audit Approved",
    );
    store.create_task(&t13).await.unwrap();
    store.add_dependency(&t13.id, &t11.id).await.unwrap();
    store.add_dependency(&t13.id, &t12.id).await.unwrap();

    let all_task_ids = [
        t1.id, t2.id, t3.id, t4.id, t5.id, t6.id, t7.id, t8.id, t9.id, t10.id, t11.id, t12.id,
        t13.id,
    ];
    let total_tasks_count = all_task_ids.len();

    // 4. Run Stress Execution Loop & Collect Metrics
    let start_time = Instant::now();
    let mut total_ticks = 0;
    let mut peak_concurrency = 0;

    loop {
        total_ticks += 1;
        let dispatched = scheduler.tick().await.unwrap();
        if dispatched > peak_concurrency {
            peak_concurrency = dispatched;
        }

        let tasks = store.list_tasks_by_workflow(&wf.id).await.unwrap();
        let verified_count = tasks
            .iter()
            .filter(|t| t.state == TaskState::Verified)
            .count();

        if verified_count == total_tasks_count {
            break;
        }

        if dispatched == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            if total_ticks > 50 {
                panic!(
                    "Scheduler stalled after {} ticks! Incomplete tasks: {:?}",
                    total_ticks,
                    tasks
                        .iter()
                        .filter(|t| t.state != TaskState::Verified)
                        .map(|t| (t.id, t.objective.clone(), t.state, t.attempts))
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    let total_duration = start_time.elapsed();
    let duration_secs = total_duration.as_secs_f64();
    let throughput = (total_tasks_count as f64) / duration_secs;

    // 5. Query durable events from SQLite for independent metrics validation
    let all_events = store.list_recent_events(1000).await.unwrap();
    let provider_req_events = all_events
        .iter()
        .filter(|e| e.event_type == "provider_request_started")
        .count();
    let tool_comp_events = all_events
        .iter()
        .filter(|e| e.event_type == "tool_completed")
        .count();
    let recovery_records = store
        .list_recovery_records_by_workflow(&wf.id)
        .await
        .unwrap();
    let recovery_events = recovery_records.len();
    let message_events = all_events
        .iter()
        .filter(|e| e.event_type == "message_sent")
        .count();
    let memory_events = all_events
        .iter()
        .filter(|e| e.event_type == "memory_saved")
        .count();

    // 6. Assertions
    // All 13 tasks must be strictly Verified
    for tid in &all_task_ids {
        let t = store.get_task(tid).await.unwrap().unwrap();
        assert_eq!(
            t.state,
            TaskState::Verified,
            "Task {} ({}) failed to reach Verified state",
            t.id,
            t.objective
        );
    }

    // T10 bottleneck had deliberate failure and recovered on attempt 2
    let t10_record = store.get_task(&t10.id).await.unwrap().unwrap();
    assert_eq!(
        t10_record.attempts, 2,
        "Task 10 should have executed twice due to recovery"
    );
    assert!(
        t10_record.metadata.get("recovery_advice").is_some(),
        "Task 10 should contain mutated strategy recovery advice"
    );

    // Peak concurrency reached at least 3
    assert!(
        peak_concurrency >= 3,
        "Peak concurrency should be at least 3, got {}",
        peak_concurrency
    );

    // Inter-agent message and memory events were recorded
    assert!(message_events >= 1, "Expected at least 1 agent message");
    assert!(
        memory_events >= 2,
        "Expected at least 2 persistent memory events"
    );
    assert!(recovery_events >= 1, "Expected at least 1 recovery event");

    // 7. Output structured baseline metrics
    println!("\n========================================================");
    println!("PLEXIS RUNTIME STRESS TEST — BASELINE EXECUTION METRICS");
    println!("========================================================");
    println!("Total Tasks Executed & Verified: {}", total_tasks_count);
    println!("Dependency Tiers:                6");
    println!("Concurrent Agent Roles:          3 (Dev, QA, Ops)");
    println!(
        "Total Execution Duration:        {:.3}s ({} ms)",
        duration_secs,
        total_duration.as_millis()
    );
    println!(
        "Task Throughput:                 {:.2} tasks/sec",
        throughput
    );
    println!(
        "Peak Concurrency:                {} concurrent tasks/tick",
        peak_concurrency
    );
    println!(
        "Tool Invocations Count:          {} completed",
        tool_comp_events
    );
    println!(
        "Provider Requests Count:         {} requests",
        provider_req_events
    );
    println!(
        "Durable Recoveries Count:        {} recovered",
        recovery_events
    );
    println!("Agent Messages Exchanged:        {}", message_events);
    println!("Persistent Memories Saved:       {}", memory_events);
    println!("========================================================\n");
}
