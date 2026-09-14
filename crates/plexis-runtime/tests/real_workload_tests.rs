use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, WorkflowId};
use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::{Agent, ExecutionProfile, VerificationVerdict, Workflow};
use plexis_planner::applier::PlanApplier;
use plexis_planner::proposal::{PlanProposal, ProposedTask};
use plexis_planner::PlanningContext;
use plexis_providers::{
    CompletionResponse, FailoverRouter, ProviderDescriptor, ProviderHealthTracker, RetryPolicy,
    ScriptedProvider, ToolCall,
};
use plexis_runtime::dispatcher::BroadcastCommandDispatcher;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::reconciler::Reconciler;
use plexis_runtime::recovery::RecoveryController;
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::scheduler::DeterministicScheduler;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::traits::{
    AgentStore, EventStore, LeaseStore, MemoryStore, MessageStore, PlanStore, RecoveryStore,
    SessionStore, TaskStore, VerificationStore, WorkflowStore,
};
use plexis_storage::SqliteStore;
use plexis_tools::builtin::{FilesystemTool, GitTool, ShellTool};
use plexis_tools::ToolRegistry;

/// Helper function to initialize a real local Git repository with an existing Rust crate.
fn setup_real_git_repository(repo_dir: &Path) {
    fs::create_dir_all(repo_dir.join("src")).unwrap();
    fs::create_dir_all(repo_dir.join("tests")).unwrap();

    // 1. Cargo.toml
    let cargo_toml = r#"[package]
name = "plexis-cache"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
    fs::write(repo_dir.join("Cargo.toml"), cargo_toml).unwrap();

    // 2. Initial src/lib.rs (intentionally lacks TTL expiration)
    let lib_rs = r#"use std::collections::HashMap;

pub struct PlexisCache {
    capacity: usize,
    entries: HashMap<String, String>,
}

impl PlexisCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    pub fn put(&mut self, key: impl Into<String>, value: impl Into<String>) {
        if self.entries.len() >= self.capacity {
            if let Some(first_key) = self.entries.keys().next().cloned() {
                self.entries.remove(&first_key);
            }
        }
        self.entries.insert(key.into(), value.into());
    }
}
"#;
    fs::write(repo_dir.join("src/lib.rs"), lib_rs).unwrap();

    // 3. Initial tests/cache_tests.rs
    let cache_tests = r#"use plexis_cache::PlexisCache;

#[test]
fn test_basic_cache() {
    let mut cache = PlexisCache::new(2);
    cache.put("a", "1");
    assert_eq!(cache.get("a"), Some("1"));
}
"#;
    fs::write(repo_dir.join("tests/cache_tests.rs"), cache_tests).unwrap();

    // 4. Initial README.md
    let readme = "# PlexisCache\nA lightweight in-memory cache in Rust.\n";
    fs::write(repo_dir.join("README.md"), readme).unwrap();

    // Initialize Git repo & initial commit
    let run_git = |args: &[&str]| {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(repo_dir)
            .status()
            .expect("git execution failed");
        assert!(status.success(), "git {:?} failed", args);
    };

    run_git(&["init"]);
    run_git(&["config", "user.name", "Test User"]);
    run_git(&["config", "user.email", "test@plexis.local"]);
    run_git(&["add", "."]);
    run_git(&["commit", "-m", "Initial baseline commit"]);
}

#[tokio::test]
async fn test_milestone5_autonomous_software_workload_end_to_end() {
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().to_path_buf();
    setup_real_git_repository(&repo_dir);

    // 1. Setup durable storage, execution backend, tools, verifier, lease manager, dispatcher
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let lease_manager = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let recovery_controller = Arc::new(RecoveryController::new(store.clone(), 3));

    let mut tool_registry = ToolRegistry::new();
    tool_registry.register(Arc::new(FilesystemTool));
    tool_registry.register(Arc::new(ShellTool));
    tool_registry.register(Arc::new(GitTool));

    // 2. High-Level Objective
    let workflow_id = WorkflowId::new();
    let mut workflow = Workflow::new(
        "Autonomous TTL Expiry Feature",
        "Add time-to-live (TTL) expiration support to plexis-cache repository, ensuring expired keys return None and can be cleaned up, write comprehensive unit tests, and commit changes with git.",
    );
    workflow.id = workflow_id;
    workflow.state = WorkflowState::Active;
    store.create_workflow(&workflow).await.unwrap();

    // 3. Autonomous Planning: Dynamically generate task graph with dependencies
    let context = PlanningContext::new(
        workflow_id,
        "Add time-to-live (TTL) expiration support to plexis-cache repository",
    );

    let proposal = PlanProposal::new(
        "Autonomous TTL Expiry Feature",
        "Add time-to-live (TTL) expiration support to plexis-cache repository, ensuring expired keys return None and can be cleaned up, write comprehensive unit tests, and commit changes with git.",
    )
    .with_task(ProposedTask {
        temp_id: "task-1-impl".into(),
        objective: "Add put_with_ttl(key, value, ttl) and cleanup_expired() methods, checking expiration in get()".into(),
        description: Some("Implement TTL cache expiration in src/lib.rs".into()),
        criteria: vec!["file:src/lib.rs".into()],
        required_capabilities: vec!["code_modification".into(), "filesystem".into()],
        suggested_role: Some("developer".into()),
        priority: 1,
    })
    .with_task(ProposedTask {
        temp_id: "task-2-test".into(),
        objective: "Add unit tests verifying expired items return None and cleanup_expired removes them".into(),
        description: Some("Write comprehensive unit tests for TTL expiry and cleanup in tests/cache_tests.rs".into()),
        criteria: vec!["file:tests/cache_tests.rs".into()],
        required_capabilities: vec!["testing".into(), "filesystem".into()],
        suggested_role: Some("qa_tester".into()),
        priority: 1,
    })
    .with_task(ProposedTask {
        temp_id: "task-3-integrate".into(),
        objective: "Verify cargo test passes, update README.md, and create git commit".into(),
        description: Some("Review, run test suite, and commit changes with git".into()),
        criteria: vec!["file:README.md".into(), "command:cargo test".into()],
        required_capabilities: vec!["git_ops".into(), "review".into()],
        suggested_role: Some("release_engineer".into()),
        priority: 2,
    })
    .with_dependency("task-3-integrate", "task-1-impl")
    .with_dependency("task-3-integrate", "task-2-test");

    let applier = PlanApplier::new(store.clone());
    let apply_res = applier
        .apply(
            &context,
            &proposal,
            "scripted",
            "mock-v1",
            10,
            Some(100),
            Some(100),
        )
        .await
        .expect("apply plan");

    let task1_id = apply_res.created_tasks["task-1-impl"];
    let task2_id = apply_res.created_tasks["task-2-test"];
    let task3_id = apply_res.created_tasks["task-3-integrate"];

    // Configure exact verification metadata on tasks
    let mut t1 = store.get_task(&task1_id).await.unwrap().unwrap();
    t1.metadata["verification"] = serde_json::json!({
        "file_path": "src/lib.rs",
        "expected_content": "pub fn put_with_ttl",
    });
    store.update_task(&t1).await.unwrap();

    let mut t2 = store.get_task(&task2_id).await.unwrap().unwrap();
    t2.metadata["verification"] = serde_json::json!({
        "file_path": "tests/cache_tests.rs",
        "expected_content": "test_ttl_expiry",
    });
    store.update_task(&t2).await.unwrap();

    let mut t3 = store.get_task(&task3_id).await.unwrap().unwrap();
    t3.metadata["verification"] = serde_json::json!({
        "command": "cargo test",
        "expected_exit_code": 0,
        "file_path": "README.md",
        "expected_content": "TTL expiration",
    });
    store.update_task(&t3).await.unwrap();

    // 4. Agent Specialization: Register specialized agents with distinct capabilities
    let dev_agent_id = AgentId::new();
    let mut dev_agent = Agent::new(
        "DeveloperAgent",
        "developer",
        ExecutionProfile::new("scripted-dev", "dev-model"),
    )
    .with_capabilities(vec![
        "code_modification".into(),
        "filesystem".into(),
        "shell".into(),
    ]);
    dev_agent.id = dev_agent_id;
    store.create_agent(&dev_agent).await.unwrap();

    let test_agent_id = AgentId::new();
    let mut test_agent = Agent::new(
        "TestingAgent",
        "qa_tester",
        ExecutionProfile::new("scripted-test", "test-model"),
    )
    .with_capabilities(vec!["testing".into(), "filesystem".into(), "shell".into()]);
    test_agent.id = test_agent_id;
    store.create_agent(&test_agent).await.unwrap();

    let release_agent_id = AgentId::new();
    let mut release_agent = Agent::new(
        "ReleaseAgent",
        "release_engineer",
        ExecutionProfile::new("failover", "release-model"),
    )
    .with_capabilities(vec!["git_ops".into(), "review".into(), "git".into()]);
    release_agent.id = release_agent_id;
    store.create_agent(&release_agent).await.unwrap();

    // 5. Providers Setup: Primary and Fallback with Circuit-Breaker Failover
    let dev_provider = Arc::new(ScriptedProvider::new("scripted-dev"));
    let test_provider = Arc::new(ScriptedProvider::new("scripted-test"));
    let release_primary = Arc::new(ScriptedProvider::new("primary-release"));
    let release_fallback = Arc::new(ScriptedProvider::new("fallback-release"));

    let primary_desc = ProviderDescriptor::new("primary-release", "gpt-4o", 1, true, false);
    let fallback_desc = ProviderDescriptor::new("fallback-release", "claude-3-5", 1, true, false);
    let health_tracker = Arc::new(ProviderHealthTracker::new(1, Duration::from_secs(60)));
    let retry_policy = RetryPolicy::new(0, Duration::from_millis(5), Duration::from_millis(20));
    let release_router = Arc::new(FailoverRouter::new(
        vec![
            (primary_desc, release_primary.clone()),
            (fallback_desc, release_fallback.clone()),
        ],
        health_tracker.clone(),
        retry_policy,
    ));

    // Script Task 1 (DevAgent on dev_provider):
    // Turn 1: Save memory with project API constraint
    dev_provider.queue_response(CompletionResponse::tool_calls(vec![
        ToolCall::new(
            "call_mem_1",
            "save_memory",
            serde_json::json!({
                "scope": "project",
                "content": "PlexisCache API constraint: TTL method is put_with_ttl(key: &str, value: &str, ttl: std::time::Duration). Key cleanup is cleanup_expired() -> usize.",
                "importance": 0.95
            }).to_string(),
        ),
    ]));
    // Turn 2: Send durable message to TestingAgent announcing the API change
    dev_provider.queue_response(CompletionResponse::tool_calls(vec![
        ToolCall::new(
            "call_msg_1",
            "send_message",
            serde_json::json!({
                "to_agent": test_agent_id.to_string(),
                "message_type": "handoff",
                "content": "Implemented TTL API: method is put_with_ttl(key, value, duration) and cleanup_expired()",
            }).to_string(),
        ),
    ]));
    // Turn 3: Deliberate failure injection: write incomplete src/lib.rs missing expected method name to cause verification failure
    dev_provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_fs_fail",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "src/lib.rs",
            "content": "// Incomplete implementation\npub struct PlexisCache;\n",
        })
        .to_string(),
    )]));
    // Turn 4: Close turn for attempt 1
    dev_provider.queue_response(CompletionResponse::text("Initial implementation written"));

    // Task 1 Attempt 2 (Recovery after diagnosis and strategy mutation):
    // Real, full, working TTL implementation in src/lib.rs
    let working_lib_rs = r#"use std::collections::HashMap;
use std::time::{Duration, Instant};

struct CacheEntry {
    value: String,
    expires_at: Option<Instant>,
}

pub struct PlexisCache {
    capacity: usize,
    entries: HashMap<String, CacheEntry>,
}

impl PlexisCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
        }
    }

    pub fn put(&mut self, key: &str, value: &str) {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(key) {
            if let Some(first_key) = self.entries.keys().next().cloned() {
                self.entries.remove(&first_key);
            }
        }
        self.entries.insert(key.to_string(), CacheEntry {
            value: value.to_string(),
            expires_at: None,
        });
    }

    pub fn put_with_ttl(&mut self, key: &str, value: &str, ttl: Duration) {
        if self.entries.len() >= self.capacity && !self.entries.contains_key(key) {
            if let Some(first_key) = self.entries.keys().next().cloned() {
                self.entries.remove(&first_key);
            }
        }
        self.entries.insert(key.to_string(), CacheEntry {
            value: value.to_string(),
            expires_at: Some(Instant::now() + ttl),
        });
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        let entry = self.entries.get(key)?;
        if let Some(expires_at) = entry.expires_at {
            if Instant::now() >= expires_at {
                return None;
            }
        }
        Some(entry.value.as_str())
    }

    pub fn cleanup_expired(&mut self) -> usize {
        let now = Instant::now();
        let initial_len = self.entries.len();
        self.entries.retain(|_, entry| {
            if let Some(expires_at) = entry.expires_at {
                expires_at > now
            } else {
                true
            }
        });
        initial_len - self.entries.len()
    }
}
"#;
    dev_provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_fs_recovery",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "src/lib.rs",
            "content": working_lib_rs,
        })
        .to_string(),
    )]));
    dev_provider.queue_response(CompletionResponse::text(
        "Recovered and implemented full TTL cache API",
    ));

    // Script Task 2 (TestingAgent on test_provider):
    // Turn 1: Search memory to confirm discovered constraint
    test_provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_search_mem",
        "search_memory",
        serde_json::json!({ "query": "put_with_ttl", "scope": "project" }).to_string(),
    )]));
    // Turn 2: Write comprehensive unit tests in tests/cache_tests.rs
    let test_file_content = r#"use plexis_cache::PlexisCache;
use std::time::Duration;
use std::thread::sleep;

#[test]
fn test_basic_cache_operations() {
    let mut cache = PlexisCache::new(2);
    cache.put("k1", "v1");
    assert_eq!(cache.get("k1"), Some("v1"));
}

#[test]
fn test_ttl_expiry() {
    let mut cache = PlexisCache::new(5);
    cache.put_with_ttl("temp", "temp_val", Duration::from_millis(50));
    assert_eq!(cache.get("temp"), Some("temp_val"));

    sleep(Duration::from_millis(70));
    assert_eq!(cache.get("temp"), None, "Expired entry must return None");

    let cleaned = cache.cleanup_expired();
    assert_eq!(cleaned, 1, "Should cleanup 1 expired entry");
}
"#;
    test_provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_fs_test",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "tests/cache_tests.rs",
            "content": test_file_content,
        })
        .to_string(),
    )]));
    // Turn 3: Send confirmation message back to DevAgent
    test_provider.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_msg_ack",
        "send_message",
        serde_json::json!({
            "to_agent": dev_agent_id.to_string(),
            "message_type": "review",
            "content": "Tests written and verified for TTL expiry and cleanup",
        })
        .to_string(),
    )]));
    test_provider.queue_response(CompletionResponse::text("Unit tests written and verified"));

    // Script Task 3 (ReleaseAgent) with Provider Outage & Failover:
    // Primary provider returns 503 Service Unavailable -> Fallback provider completes the task!
    release_primary.queue_error("503 Service Unavailable: Rate limited");

    // Fallback provider queue:
    // Turn 1: Update README.md with documentation
    let updated_readme = "# PlexisCache\nA lightweight in-memory cache in Rust.\n\n## Features\n- LRU capacity eviction\n- TTL expiration with `put_with_ttl` and `cleanup_expired`\n";
    release_fallback.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_fb_readme",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "README.md",
            "content": updated_readme,
        })
        .to_string(),
    )]));
    // Turn 2: Run git status
    release_fallback.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_fb_git_status",
        "git",
        serde_json::json!({ "action": "status" }).to_string(),
    )]));
    // Turn 3: Run git diff
    release_fallback.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_fb_git_diff",
        "git",
        serde_json::json!({ "action": "diff" }).to_string(),
    )]));
    // Turn 4: Create git commit
    release_fallback.queue_response(CompletionResponse::tool_calls(vec![ToolCall::new(
        "call_fb_git_commit",
        "git",
        serde_json::json!({
            "action": "commit",
            "message": "feat: implement TTL expiration and cleanup in plexis-cache"
        })
        .to_string(),
    )]));
    release_fallback.queue_response(CompletionResponse::text(
        "Release integration verified and committed",
    ));

    // 6. Build Runner & Scheduler
    let mut runner = AgentRunner::new(store.clone(), tool_registry, verifier.clone())
        .with_recovery_controller(recovery_controller.clone());
    runner.register_provider(dev_provider.clone());
    runner.register_provider(test_provider.clone());
    runner.register_provider(release_router.clone());
    let runner = Arc::new(runner);

    let scheduler = DeterministicScheduler::new(
        store.clone(),
        lease_manager.clone(),
        dispatcher.clone(),
        runner.clone(),
    );

    // Set working directory to repo_dir for sessions
    // Initial session for dev agent
    let mut dev_session = plexis_core::Session::new(dev_agent_id);
    dev_session.working_directory = Some(repo_dir.display().to_string());
    store.create_session(&dev_session).await.unwrap();

    let mut test_session = plexis_core::Session::new(test_agent_id);
    test_session.working_directory = Some(repo_dir.display().to_string());
    store.create_session(&test_session).await.unwrap();

    let mut release_session = plexis_core::Session::new(release_agent_id);
    release_session.working_directory = Some(repo_dir.display().to_string());
    store.create_session(&release_session).await.unwrap();

    // -------------------------------------------------------------
    // PHASE 1: Parallel Tick: Task 1 and Task 2 run concurrently!
    // -------------------------------------------------------------
    let dispatched = scheduler.tick().await.expect("scheduler tick 1");
    assert_eq!(
        dispatched, 2,
        "Task 1 (Dev) and Task 2 (Test) must be dispatched concurrently in parallel"
    );

    // Task 1 initial attempt failed due to deliberate bug, but RecoveryController caught it and mutated strategy to Ready!
    let t1_state = store.get_task(&task1_id).await.unwrap().unwrap().state;
    assert_eq!(
        t1_state,
        TaskState::Ready,
        "Task 1 must be reset to Ready by RecoveryController for retry"
    );

    // Task 2 completed cleanly and is Verified!
    let t2_state = store.get_task(&task2_id).await.unwrap().unwrap().state;
    assert_eq!(t2_state, TaskState::Verified, "Task 2 must be Verified");

    // -------------------------------------------------------------
    // PHASE 2: Mid-Execution Interruption & Restart Simulation
    // -------------------------------------------------------------
    // Simulate sudden crash: drop scheduler and runner.
    drop(scheduler);
    drop(runner);

    // Startup Reconciler runs on new instance
    let reconciler = Reconciler::new(
        store.clone() as Arc<dyn TaskStore>,
        store.clone() as Arc<dyn LeaseStore>,
        store.clone() as Arc<dyn EventStore>,
    )
    .with_workflow_store(store.clone() as Arc<dyn WorkflowStore>);

    let startup_report = reconciler
        .reconcile_startup()
        .await
        .expect("startup reconcile");
    assert!(startup_report.resumable_workflows.contains(&workflow_id));

    // Recreate runner and scheduler after restart
    let mut tool_registry_restart = ToolRegistry::new();
    tool_registry_restart.register(Arc::new(FilesystemTool));
    tool_registry_restart.register(Arc::new(ShellTool));
    tool_registry_restart.register(Arc::new(GitTool));

    let mut runner_restart =
        AgentRunner::new(store.clone(), tool_registry_restart, verifier.clone())
            .with_recovery_controller(recovery_controller.clone());
    runner_restart.register_provider(dev_provider.clone());
    runner_restart.register_provider(test_provider.clone());
    runner_restart.register_provider(release_router.clone());
    let runner_restart = Arc::new(runner_restart);

    let scheduler_restart = DeterministicScheduler::new(
        store.clone(),
        lease_manager.clone(),
        dispatcher.clone(),
        runner_restart.clone(),
    );

    // -------------------------------------------------------------
    // PHASE 3: Retry Task 1 (Recovery attempt)
    // -------------------------------------------------------------
    let dispatched_retry = scheduler_restart
        .tick()
        .await
        .expect("scheduler retry tick");
    assert_eq!(
        dispatched_retry, 1,
        "Task 1 retry attempt must be dispatched"
    );

    let t1_final_state = store.get_task(&task1_id).await.unwrap().unwrap().state;
    assert_eq!(
        t1_final_state,
        TaskState::Verified,
        "Task 1 must be Verified after recovery"
    );

    // -------------------------------------------------------------
    // PHASE 4: Dependency Bottleneck Passed: Task 3 (Release) is now Runnable!
    // -------------------------------------------------------------
    // Both Task 1 and Task 2 are Verified, so Task 3 should run now
    let dispatched_release = scheduler_restart
        .tick()
        .await
        .expect("scheduler release tick");
    assert_eq!(
        dispatched_release, 1,
        "Task 3 (Release Integration) must be dispatched"
    );

    let t3_final_state = store.get_task(&task3_id).await.unwrap().unwrap().state;
    assert_eq!(
        t3_final_state,
        TaskState::Verified,
        "Task 3 must be Verified"
    );

    // -------------------------------------------------------------
    // PHASE 5: Authoritative Verification & Real Repository Assertions
    // -------------------------------------------------------------
    // 1. Check real Git log contains new commit created by Plexis tooling
    let git_log_out = StdCommand::new("git")
        .args(["log", "--oneline", "-n", "3"])
        .current_dir(&repo_dir)
        .output()
        .expect("git log");
    let log_str = String::from_utf8_lossy(&git_log_out.stdout);
    assert!(
        log_str.contains("feat: implement TTL expiration and cleanup in plexis-cache"),
        "Repository git log must contain commit created by Plexis, got:\n{}",
        log_str
    );

    // 2. Execute `cargo test` independently in the repository
    let cargo_test_res = StdCommand::new("cargo")
        .arg("test")
        .current_dir(&repo_dir)
        .output()
        .expect("cargo test");
    assert!(
        cargo_test_res.status.success(),
        "All real Rust tests in repository must pass! Stderr:\n{}",
        String::from_utf8_lossy(&cargo_test_res.stderr)
    );

    // 3. Inspect durable Memory records
    let project_memories = store
        .list_memories_by_scope(plexis_core::MemoryScope::Project, None)
        .await
        .expect("list project memories");
    assert!(
        !project_memories.is_empty(),
        "Project memory must be durably persisted"
    );
    assert!(project_memories[0].content.contains("put_with_ttl"));

    // 4. Inspect durable Agent Messages
    let messages = store
        .list_messages_by_workflow(&workflow_id)
        .await
        .expect("list messages");
    assert!(
        messages.len() >= 2,
        "Must contain coordination exchange messages between agents"
    );

    // 5. Inspect durable Recovery records
    let recovery_records = store
        .list_recovery_records_by_task(&task1_id)
        .await
        .expect("list recovery records");
    assert_eq!(
        recovery_records.len(),
        1,
        "Must contain durable recovery record for Task 1 failure"
    );
    assert_eq!(recovery_records[0].strategy, "prompt_refinement");

    // 6. Inspect durable Planning record
    let plan_records = store
        .list_plan_records_by_workflow(&workflow_id)
        .await
        .expect("list plans");
    assert_eq!(
        plan_records.len(),
        1,
        "Must contain persisted planning record"
    );
    assert_eq!(plan_records[0].status, plexis_core::PlanStatus::Applied);

    // 7. Inspect Verifications
    let v1 = store.list_verifications_by_task(&task1_id).await.unwrap();
    assert!(
        v1.iter().any(|v| v.verdict == VerificationVerdict::Failed),
        "Task 1 must have recorded initial failure"
    );
    assert!(
        v1.iter().any(|v| v.verdict == VerificationVerdict::Passed),
        "Task 1 must have recorded passed recovery"
    );

    let v3 = store.list_verifications_by_task(&task3_id).await.unwrap();
    assert_eq!(v3.len(), 1);
    assert_eq!(v3[0].verdict, VerificationVerdict::Passed);
    assert_eq!(v3[0].evidence["command"]["exit_code"], 0);

    // 8. Verify Provider Circuit-Breaker Failover Health
    assert!(
        !health_tracker.is_available("primary-release"),
        "Primary release provider should be marked unavailable after failure"
    );
    assert!(
        health_tracker.is_available("fallback-release"),
        "Fallback release provider should be available and active"
    );
}
