use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::tempdir;

use async_trait::async_trait;
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
use plexis_storage::traits::{AgentStore, SessionStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;
use plexis_tools::builtin::FilesystemTool;
use plexis_tools::ToolRegistry;

/// Deterministic provider serving parallel execution for 25+ task workloads.
struct LargeScaleProvider {
    pub requests_count: AtomicUsize,
}

impl LargeScaleProvider {
    pub fn new() -> Self {
        Self {
            requests_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Provider for LargeScaleProvider {
    fn id(&self) -> &str {
        "large_scale_provider"
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        self.requests_count.fetch_add(1, Ordering::SeqCst);

        // If last message is Tool response, return completion text
        if let Some(last_msg) = request.messages.last() {
            if last_msg.role == ChatRole::Tool {
                return Ok(CompletionResponse::text("Task execution complete."));
            }
        }

        let all_text = request
            .messages
            .iter()
            .filter_map(|m| m.content.as_deref())
            .collect::<Vec<_>>()
            .join("\n");

        for i in 1..=25 {
            let marker = format!("Task {} ", i);
            if all_text.contains(&marker) {
                let path = format!("scale_file_{}.txt", i);
                let content = format!("content_{}", i);
                return Ok(CompletionResponse::tool_calls(vec![ToolCall::new(
                    format!("call_{}", i),
                    "filesystem",
                    serde_json::json!({
                        "action": "write_file",
                        "path": path,
                        "content": content
                    })
                    .to_string(),
                )]));
            }
        }

        Ok(CompletionResponse::text("Default completion"))
    }
}

#[tokio::test]
async fn test_large_scale_25_task_workflow_with_5_parallel_agents_and_latency_metrics() {
    let temp = tempdir().expect("tempdir");
    let repo_dir = temp.path().to_path_buf();

    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));
    let lease_mgr = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(500));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let recovery_controller = Arc::new(RecoveryController::new(store.clone(), 3));

    // 1. Create specialized agents with diverse capabilities
    let roles = [
        (
            "PlannerAgent",
            "planner",
            vec!["planner".into(), "filesystem".into()],
        ),
        (
            "DevAgent",
            "developer",
            vec!["dev".into(), "filesystem".into()],
        ),
        (
            "DevAgent2",
            "developer",
            vec!["dev".into(), "filesystem".into()],
        ),
        (
            "TesterAgent",
            "tester",
            vec!["test".into(), "filesystem".into()],
        ),
        (
            "TesterAgent2",
            "tester",
            vec!["test".into(), "filesystem".into()],
        ),
        (
            "ReviewerAgent",
            "reviewer",
            vec!["review".into(), "filesystem".into()],
        ),
        (
            "VerifierAgent",
            "verifier",
            vec!["verify".into(), "filesystem".into()],
        ),
    ];

    let mut agent_ids = Vec::new();
    for (name, role, caps) in roles {
        let agent = Agent::new(
            name,
            role,
            ExecutionProfile::new("large_scale_provider", "scale-model"),
        )
        .with_capabilities(caps);
        store.create_agent(&agent).await.expect("create agent");

        let mut session = Session::new(agent.id);
        session.working_directory = Some(repo_dir.display().to_string());
        store
            .create_session(&session)
            .await
            .expect("create session");
        agent_ids.push(agent.id);
    }

    assert!(agent_ids.len() >= 5);

    // 2. Setup Tool Registry and Runner
    let mut tool_reg = ToolRegistry::new();
    tool_reg.register(Arc::new(FilesystemTool));

    let provider = Arc::new(LargeScaleProvider::new());
    let mut runner = AgentRunner::new(store.clone(), tool_reg, verifier)
        .with_recovery_controller(recovery_controller);
    runner.register_provider(provider.clone());
    let runner = Arc::new(runner);

    let scheduler = DeterministicScheduler::new(
        store.clone(),
        lease_mgr.clone(),
        dispatcher.clone(),
        runner.clone(),
    );

    // 3. Construct 25-task fan-out / fan-in DAG
    let mut wf = Workflow::new(
        "WF-Scale-25",
        "25-task fan-out/fan-in scalability benchmark",
    );
    wf.state = WorkflowState::Active;
    store.create_workflow(&wf).await.expect("create workflow");

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

    // Tier 1: 1 root task
    let t1 = make_task(
        "Task 1 (Architecture & System Design)",
        "planner",
        "planner",
        "scale_file_1.txt",
        "content_1",
    );
    store.create_task(&t1).await.expect("create t1");

    // Tier 2: 5 parallel fan-out tasks from T1 (Core, Auth, API, Cache, Storage)
    let tier2_roles = ["planner", "developer", "tester", "reviewer", "verifier"];
    let tier2_caps = ["planner", "dev", "test", "review", "verify"];
    let mut tier2_tasks = Vec::new();
    for i in 2..=6 {
        let t = make_task(
            &format!("Task {} (Module {})", i, i),
            tier2_roles[i - 2],
            tier2_caps[i - 2],
            &format!("scale_file_{}.txt", i),
            &format!("content_{}", i),
        );
        store.create_task(&t).await.expect("create tier2 task");
        store
            .add_dependency(&t.id, &t1.id)
            .await
            .expect("dep on t1");
        tier2_tasks.push(t);
    }

    // Tier 3: 5 parallel tasks (Unit tests for each module)
    let mut tier3_tasks = Vec::new();
    for i in 7..=11 {
        let parent_idx = i - 7;
        let t = make_task(
            &format!("Task {} (Unit Tests for Module {})", i, parent_idx + 2),
            "tester",
            "test",
            &format!("scale_file_{}.txt", i),
            &format!("content_{}", i),
        );
        store.create_task(&t).await.expect("create tier3 task");
        store
            .add_dependency(&t.id, &tier2_tasks[parent_idx].id)
            .await
            .expect("dep on tier2");
        tier3_tasks.push(t);
    }

    // Tier 4: 5 parallel tasks (Integration & Concurrency checks)
    let mut tier4_tasks = Vec::new();
    for i in 12..=16 {
        let parent_idx = i - 12;
        let t = make_task(
            &format!("Task {} (Integration Check {})", i, i),
            "developer",
            "dev",
            &format!("scale_file_{}.txt", i),
            &format!("content_{}", i),
        );
        store.create_task(&t).await.expect("create tier4 task");
        store
            .add_dependency(&t.id, &tier3_tasks[parent_idx].id)
            .await
            .expect("dep on tier3");
        tier4_tasks.push(t);
    }

    // Tier 5: 1 fan-in bottleneck task (Audit Gateway) depending on all Tier 4 tasks
    let t17 = make_task(
        "Task 17 (Audit Gateway & Security Baseline)",
        "verifier",
        "verify",
        "scale_file_17.txt",
        "content_17",
    );
    store.create_task(&t17).await.expect("create t17");
    for t4 in &tier4_tasks {
        store
            .add_dependency(&t17.id, &t4.id)
            .await
            .expect("dep on t4");
    }

    // Tier 6: 5 parallel fan-out tasks from T17
    let mut tier6_tasks = Vec::new();
    for i in 18..=22 {
        let t = make_task(
            &format!("Task {} (Release Artifact {})", i, i),
            tier2_roles[i - 18],
            tier2_caps[i - 18],
            &format!("scale_file_{}.txt", i),
            &format!("content_{}", i),
        );
        store.create_task(&t).await.expect("create tier6 task");
        store
            .add_dependency(&t.id, &t17.id)
            .await
            .expect("dep on t17");
        tier6_tasks.push(t);
    }

    // Tier 7: 2 parallel tasks from Tier 6
    let t23 = make_task(
        "Task 23 (Regression Suite Verification)",
        "tester",
        "test",
        "scale_file_23.txt",
        "content_23",
    );
    store.create_task(&t23).await.expect("create t23");
    for t in &tier6_tasks[0..3] {
        store
            .add_dependency(&t23.id, &t.id)
            .await
            .expect("dep on t");
    }

    let t24 = make_task(
        "Task 24 (Compliance & Policy Sign-off)",
        "reviewer",
        "review",
        "scale_file_24.txt",
        "content_24",
    );
    store.create_task(&t24).await.expect("create t24");
    for t in &tier6_tasks[3..5] {
        store
            .add_dependency(&t24.id, &t.id)
            .await
            .expect("dep on t");
    }

    // Tier 8: 1 terminal task
    let t25 = make_task(
        "Task 25 (Final Verification & Production Release)",
        "verifier",
        "verify",
        "scale_file_25.txt",
        "content_25",
    );
    store.create_task(&t25).await.expect("create t25");
    store
        .add_dependency(&t25.id, &t23.id)
        .await
        .expect("dep on t23");
    store
        .add_dependency(&t25.id, &t24.id)
        .await
        .expect("dep on t24");

    // Total tasks = 25
    let tasks = store
        .list_tasks_by_workflow(&wf.id)
        .await
        .expect("list tasks");
    assert_eq!(tasks.len(), 25);

    // 4. Execution Loop with Latency Profiling
    let overall_start = Instant::now();
    let mut tick_latencies: Vec<Duration> = Vec::new();
    let mut db_latencies: Vec<Duration> = Vec::new();
    let mut peak_concurrency = 0;
    let mut tick_count = 0;

    loop {
        tick_count += 1;

        let tick_start = Instant::now();
        let dispatched = scheduler.tick().await.expect("scheduler tick");
        let tick_elapsed = tick_start.elapsed();
        tick_latencies.push(tick_elapsed);

        if dispatched > peak_concurrency {
            peak_concurrency = dispatched;
        }

        let db_start = Instant::now();
        let current_tasks = store
            .list_tasks_by_workflow(&wf.id)
            .await
            .expect("list tasks");
        let db_elapsed = db_start.elapsed();
        db_latencies.push(db_elapsed);

        let verified_count = current_tasks
            .iter()
            .filter(|t| t.state == TaskState::Verified)
            .count();

        if verified_count == 25 {
            break;
        }

        if dispatched == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if tick_count > 100 {
                panic!("Scheduler stalled on 25-task workflow!");
            }
        }
    }

    let overall_duration = overall_start.elapsed();

    // 5. Latency calculations
    let avg_tick_ms = tick_latencies
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / (tick_latencies.len() as f64);
    let max_tick_ms = tick_latencies
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .fold(0.0, f64::max);

    let avg_db_ms = db_latencies
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .sum::<f64>()
        / (db_latencies.len() as f64);
    let max_db_ms = db_latencies
        .iter()
        .map(|d| d.as_secs_f64() * 1000.0)
        .fold(0.0, f64::max);

    // 6. Assertions
    assert_eq!(tasks.len(), 25);
    for t in store.list_tasks_by_workflow(&wf.id).await.unwrap() {
        assert_eq!(
            t.state,
            TaskState::Verified,
            "Task {} failed to reach Verified",
            t.id
        );
    }

    // High concurrency verified (at least 5 parallel tasks dispatched in a single tick)
    assert!(
        peak_concurrency >= 5,
        "Peak concurrency should be at least 5, got {}",
        peak_concurrency
    );

    // Latency sanity bounds
    assert!(
        avg_tick_ms < 50.0,
        "Average tick latency too high: {:.2} ms",
        avg_tick_ms
    );
    assert!(
        avg_db_ms < 20.0,
        "Average DB latency too high: {:.2} ms",
        avg_db_ms
    );

    println!("\n==================================================================");
    println!("25-TASK SCALE & LATENCY BENCHMARK RESULTS");
    println!("==================================================================");
    println!("Total Tasks Verified:       25 / 25");
    println!("Active Specialized Agents:  5 (Planner, Dev, Tester, Reviewer, Verifier)");
    println!(
        "Peak Concurrency Dispatched:{} tasks/tick",
        peak_concurrency
    );
    println!(
        "Overall Duration:           {:.2} ms",
        overall_duration.as_millis()
    );
    println!("Average Scheduler Tick:     {:.3} ms", avg_tick_ms);
    println!("Max Scheduler Tick:         {:.3} ms", max_tick_ms);
    println!("Average DB Query Latency:   {:.3} ms", avg_db_ms);
    println!("Max DB Query Latency:       {:.3} ms", max_db_ms);
    println!(
        "Provider Requests Served:   {}",
        provider.requests_count.load(Ordering::SeqCst)
    );
    println!("==================================================================\n");
}

#[tokio::test]
async fn test_database_lifecycle_backup_restore_and_restart() {
    let dir = tempdir().expect("tempdir");
    let primary_db = dir.path().join("primary.db");
    let backup_db = dir.path().join("backup.db");

    let wf_id;
    let t1_id;
    let t2_id;

    // 1. Initialize fresh primary SQLite database on disk
    {
        let store = SqliteStore::open(primary_db.to_str().unwrap()).expect("open primary store");

        let agent = Agent::new(
            "LifecycleAgent",
            "lifecycle_role",
            ExecutionProfile::new("mock", "mock-model"),
        );
        store.create_agent(&agent).await.expect("create agent");

        let wf = Workflow::new("Lifecycle-WF", "Database backup and restore test");
        wf_id = wf.id;
        store.create_workflow(&wf).await.expect("create workflow");

        let t1 = Task::new(wf_id, "Initial Task 1");
        t1_id = t1.id;
        store.create_task(&t1).await.expect("create t1");

        let t2 = Task::new(wf_id, "Initial Task 2");
        t2_id = t2.id;
        store.create_task(&t2).await.expect("create t2");
        store.add_dependency(&t2_id, &t1_id).await.expect("add dep");

        let tasks = store
            .list_tasks_by_workflow(&wf_id)
            .await
            .expect("list tasks");
        assert_eq!(tasks.len(), 2);
    }

    // 2. Perform Database Backup (file snapshot of consistent SQLite database)
    std::fs::copy(&primary_db, &backup_db).expect("copy primary to backup");
    assert!(backup_db.exists());

    // 3. Mutate Primary Database (add task 3, verify change)
    {
        let store = SqliteStore::open(primary_db.to_str().unwrap()).expect("reopen primary");
        let t3 = Task::new(wf_id, "Mutated Task 3");
        store.create_task(&t3).await.expect("create t3");

        let tasks = store
            .list_tasks_by_workflow(&wf_id)
            .await
            .expect("list tasks");
        assert_eq!(tasks.len(), 3);
    }

    // 4. Restore Database from Backup
    std::fs::copy(&backup_db, &primary_db).expect("restore backup over primary");

    // 5. Open Restored Store and Verify Accurate Historical State
    {
        let store = SqliteStore::open(primary_db.to_str().unwrap()).expect("open restored primary");
        let wf = store
            .get_workflow(&wf_id)
            .await
            .expect("get wf")
            .expect("wf exists");
        assert_eq!(wf.title, "Lifecycle-WF");

        let tasks = store
            .list_tasks_by_workflow(&wf_id)
            .await
            .expect("list tasks");
        assert_eq!(
            tasks.len(),
            2,
            "Restored database must contain exactly the 2 pre-backup tasks"
        );

        let t1 = store
            .get_task(&t1_id)
            .await
            .expect("get t1")
            .expect("t1 exists");
        assert_eq!(t1.objective, "Initial Task 1");

        let t2 = store
            .get_task(&t2_id)
            .await
            .expect("get t2")
            .expect("t2 exists");
        assert_eq!(t2.objective, "Initial Task 2");
    }
}
