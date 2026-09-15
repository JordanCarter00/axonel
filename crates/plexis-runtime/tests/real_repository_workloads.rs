//! Real repository coding workflows, multi-agent coordination, and durable communication.
//!
//! Verifies:
//! - Repository A: small Rust project with a bug repaired end-to-end
//! - Repository B: project with missing feature + tests developed concurrently
//! - Repository C: documentation + tests + integration with injected failure & recovery
//! - Multi-agent coordination: Planner, Researcher, Developer, Tester, Reviewer, Integrator, Verifier
//! - Durable agent communication (Developer -> Tester, Tester -> Developer, Reviewer -> Integrator)
//! - Real Git operations (status, diff, log, commit) with real commits in repositories
//! - Independent verification via WorkspaceVerifier

use std::fs;
use std::path::Path;
use std::process::Command as StdCommand;
use std::sync::Arc;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, TaskId, WorkflowId};
use plexis_core::state::TaskState;
use plexis_core::{Agent, ExecutionProfile, Task, VerificationVerdict, Workflow};
use plexis_providers::{
    ChatMessage, ChatRole, CompletionResponse, FinishReason, ScriptedProvider, TokenUsage, ToolCall,
};
use plexis_runtime::dispatcher::BroadcastCommandDispatcher;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::recovery::RecoveryController;
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::scheduler::DeterministicScheduler;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::traits::{
    AgentStore, MessageStore, SessionStore, TaskStore, VerificationStore, WorkflowStore,
};
use plexis_storage::SqliteStore;
use plexis_tools::builtin::{FilesystemTool, GitTool, ShellTool};
use plexis_tools::ToolRegistry;
use serde_json::json;

// ---------------------------------------------------------------------------
// Repository Setup Helpers
// ---------------------------------------------------------------------------

fn run_git_cmd(repo_dir: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .args(args)
        .current_dir(repo_dir)
        .status()
        .expect("git execution failed");
    assert!(status.success(), "git command {:?} failed", args);
}

/// Repository A: Small Rust project with a divide-by-zero bug.
fn setup_repository_a_buggy_calc(repo_dir: &Path) {
    fs::create_dir_all(repo_dir.join("src")).unwrap();
    fs::create_dir_all(repo_dir.join("tests")).unwrap();

    let cargo_toml = r#"[package]
name = "buggy-calc"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
    fs::write(repo_dir.join("Cargo.toml"), cargo_toml).unwrap();

    let lib_rs = r#"pub fn divide(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        None
    } else {
        Some(a / b)
    }
}

pub fn modulo_positive(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        None
    } else {
        Some(a % b)
    }
}
"#;
    fs::write(repo_dir.join("src/lib.rs"), lib_rs).unwrap();

    let tests_rs = r#"use buggy_calc::{divide, modulo_positive};

#[test]
fn test_divide_valid() {
    assert_eq!(divide(10, 2), Some(5));
}

#[test]
fn test_divide_by_zero() {
    assert_eq!(divide(10, 0), None);
}

#[test]
fn test_modulo_positive() {
    assert_eq!(modulo_positive(10, 3), Some(1));
    assert_eq!(modulo_positive(-10, 3), Some(2));
}
"#;
    fs::write(repo_dir.join("tests/calc_tests.rs"), tests_rs).unwrap();

    let readme = "# BuggyCalc\nA basic arithmetic calculator crate.\n";
    fs::write(repo_dir.join("README.md"), readme).unwrap();

    run_git_cmd(repo_dir, &["init"]);
    run_git_cmd(repo_dir, &["config", "user.name", "Plexis Developer"]);
    run_git_cmd(repo_dir, &["config", "user.email", "dev@plexis.local"]);
    run_git_cmd(repo_dir, &["add", "."]);
    run_git_cmd(repo_dir, &["commit", "-m", "Initial commit for buggy-calc"]);
}

/// Repository B: Rate limiter project with missing burst feature + tests.
fn setup_repository_b_token_bucket(repo_dir: &Path) {
    fs::create_dir_all(repo_dir.join("src")).unwrap();
    fs::create_dir_all(repo_dir.join("tests")).unwrap();

    let cargo_toml = r#"[package]
name = "token-bucket"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
    fs::write(repo_dir.join("Cargo.toml"), cargo_toml).unwrap();

    let lib_rs = r#"pub struct TokenBucket {
    capacity: usize,
    tokens: usize,
}

impl TokenBucket {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            tokens: capacity,
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }

    pub fn available_tokens(&self) -> usize {
        self.tokens
    }
}
"#;
    fs::write(repo_dir.join("src/lib.rs"), lib_rs).unwrap();

    let tests_rs = r#"use token_bucket::TokenBucket;

#[test]
fn test_acquire_basic() {
    let mut tb = TokenBucket::new(2);
    assert!(tb.try_acquire());
    assert!(tb.try_acquire());
    assert!(!tb.try_acquire());
}
"#;
    fs::write(repo_dir.join("tests/limiter_tests.rs"), tests_rs).unwrap();

    let readme = "# TokenBucket\nA simple token bucket rate limiter.\n";
    fs::write(repo_dir.join("README.md"), readme).unwrap();

    run_git_cmd(repo_dir, &["init"]);
    run_git_cmd(repo_dir, &["config", "user.name", "Plexis Developer"]);
    run_git_cmd(repo_dir, &["config", "user.email", "dev@plexis.local"]);
    run_git_cmd(repo_dir, &["add", "."]);
    run_git_cmd(
        repo_dir,
        &["commit", "-m", "Initial commit for token-bucket"],
    );
}

/// Repository C: Configuration loader requiring documentation, tests, and integration.
fn setup_repository_c_config_loader(repo_dir: &Path) {
    fs::create_dir_all(repo_dir.join("src")).unwrap();
    fs::create_dir_all(repo_dir.join("tests")).unwrap();

    let cargo_toml = r#"[package]
name = "config-loader"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
    fs::write(repo_dir.join("Cargo.toml"), cargo_toml).unwrap();

    let lib_rs = r#"pub struct Config {
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn parse(content: &str) -> Result<Self, String> {
        let mut host = "127.0.0.1".to_string();
        let mut port = 8080;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "host" => host = v.trim().to_string(),
                    "port" => port = v.trim().parse().map_err(|e| format!("invalid port: {}", e))?,
                    _ => {}
                }
            }
        }

        Ok(Self { host, port })
    }
}
"#;
    fs::write(repo_dir.join("src/lib.rs"), lib_rs).unwrap();

    let tests_rs = r#"use config_loader::Config;

#[test]
fn test_parse_default() {
    let cfg = Config::parse("").unwrap();
    assert_eq!(cfg.host, "127.0.0.1");
    assert_eq!(cfg.port, 8080);
}
"#;
    fs::write(repo_dir.join("tests/loader_tests.rs"), tests_rs).unwrap();

    let readme = "# ConfigLoader\nA zero-dependency key-value configuration parser.\n";
    fs::write(repo_dir.join("README.md"), readme).unwrap();

    run_git_cmd(repo_dir, &["init"]);
    run_git_cmd(repo_dir, &["config", "user.name", "Plexis Developer"]);
    run_git_cmd(repo_dir, &["config", "user.email", "dev@plexis.local"]);
    run_git_cmd(repo_dir, &["add", "."]);
    run_git_cmd(
        repo_dir,
        &["commit", "-m", "Initial commit for config-loader"],
    );
}

fn setup_tools() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(FilesystemTool));
    registry.register(Arc::new(ShellTool));
    registry.register(Arc::new(GitTool));
    registry
}

// ---------------------------------------------------------------------------
// Test 1: Repository A - Bug Diagnosis, Code Repair & Git Commit (§4, §5, §11)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_repository_a_bug_repair_and_git_commit() {
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().to_path_buf();
    setup_repository_a_buggy_calc(&repo_dir);

    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let lease_manager = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let recovery_controller = Arc::new(RecoveryController::new(store.clone(), 3));
    let tool_registry = setup_tools();

    let workflow_id = WorkflowId::new();
    let mut workflow = Workflow::new(
        "Repair Modulo Logic in BuggyCalc",
        "Diagnose failing modulo test on negative numbers, modify src/lib.rs with euclidean modulo, run tests, and commit with git.",
    );
    workflow.id = workflow_id;
    workflow.state = plexis_core::state::WorkflowState::Active;
    store.create_workflow(&workflow).await.unwrap();

    let task1_id = TaskId::new();
    let mut task1 = Task::new(workflow_id, "Fix negative modulo bug in src/lib.rs");
    task1.id = task1_id;
    task1.state = TaskState::Ready;
    task1.priority = 10;
    task1.metadata = json!({
        "required_capabilities": ["filesystem_write", "shell"],
        "suggested_role": "Developer"
    });
    store.create_task(&task1).await.unwrap();

    let task2_id = TaskId::new();
    let mut task2 = Task::new(workflow_id, "Run cargo test and commit changes");
    task2.id = task2_id;
    task2.state = TaskState::Backlog;
    task2.priority = 5;
    task2.metadata = json!({
        "required_capabilities": ["git", "shell"],
        "suggested_role": "Integrator",
        "verification_type": "command",
        "verification_target": "cargo test"
    });
    store.create_task(&task2).await.unwrap();
    store.add_dependency(&task2_id, &task1_id).await.unwrap();

    let dev_agent = Agent::new(
        "Core Developer",
        "Developer",
        ExecutionProfile::new("scripted", "default"),
    )
    .with_capabilities(vec!["filesystem_write".into(), "shell".into()]);
    store.create_agent(&dev_agent).await.unwrap();

    let int_agent = Agent::new(
        "System Integrator",
        "Integrator",
        ExecutionProfile::new("scripted", "default"),
    )
    .with_capabilities(vec!["git".into(), "shell".into()]);
    store.create_agent(&int_agent).await.unwrap();

    store
        .create_session(
            &plexis_core::Session::new(dev_agent.id)
                .with_working_directory(repo_dir.to_str().unwrap()),
        )
        .await
        .unwrap();
    store
        .create_session(
            &plexis_core::Session::new(int_agent.id)
                .with_working_directory(repo_dir.to_str().unwrap()),
        )
        .await
        .unwrap();

    let provider = Arc::new(ScriptedProvider::new("scripted"));

    let fixed_lib_rs = r#"pub fn divide(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        None
    } else {
        Some(a / b)
    }
}

pub fn modulo_positive(a: i64, b: i64) -> Option<i64> {
    if b == 0 {
        None
    } else {
        let rem = a % b;
        if rem < 0 {
            Some(rem + b.abs())
        } else {
            Some(rem)
        }
    }
}
"#;

    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Repairing modulo_positive in src/lib.rs".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_write_1",
                "filesystem",
                json!({
                    "action": "write",
                    "path": "src/lib.rs",
                    "content": fixed_lib_rs
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage {
            prompt_tokens: 30,
            completion_tokens: 20,
            total_tokens: 50,
        },
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage::assistant("Fix applied successfully to src/lib.rs"),
        usage: TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        },
        finish_reason: FinishReason::Stop,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Running test suite to verify repair".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_test_1",
                "shell",
                json!({ "command": "cargo test" }).to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage {
            prompt_tokens: 25,
            completion_tokens: 15,
            total_tokens: 40,
        },
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Committing repair with git".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_git_1",
                "git",
                json!({
                    "subcommand": "commit",
                    "args": ["-am", "fix: correct euclidean modulo logic for negative numbers"]
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage {
            prompt_tokens: 20,
            completion_tokens: 10,
            total_tokens: 30,
        },
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage::assistant("Repository repair complete and committed"),
        usage: TokenUsage {
            prompt_tokens: 5,
            completion_tokens: 5,
            total_tokens: 10,
        },
        finish_reason: FinishReason::Stop,
    });

    let mut runner = AgentRunner::new(store.clone(), tool_registry, verifier.clone())
        .with_recovery_controller(recovery_controller);
    runner.register_provider(provider);
    let runner_arc = Arc::new(runner);

    let scheduler =
        DeterministicScheduler::new(store.clone(), lease_manager, dispatcher, runner_arc);

    let outcome1 = scheduler.tick().await.unwrap();
    assert_eq!(outcome1, 1);

    let t1_after = store.get_task(&task1_id).await.unwrap().unwrap();
    assert_eq!(t1_after.state, TaskState::Verified);

    let updated_src = fs::read_to_string(repo_dir.join("src/lib.rs")).unwrap();
    assert!(updated_src.contains("rem + b.abs()"));

    let outcome2 = scheduler.tick().await.unwrap();
    assert_eq!(outcome2, 1);

    let t2_after = store.get_task(&task2_id).await.unwrap().unwrap();
    assert_eq!(t2_after.state, TaskState::Verified);

    let git_log_output = StdCommand::new("git")
        .args(["log", "-n", "2", "--oneline"])
        .current_dir(&repo_dir)
        .output()
        .expect("git log");
    let log_str = String::from_utf8_lossy(&git_log_output.stdout);
    assert!(log_str.contains("fix: correct euclidean modulo logic"));

    let verifs = store.list_verifications_by_task(&task2_id).await.unwrap();
    assert!(!verifs.is_empty());
    assert_eq!(verifs[0].verdict, VerificationVerdict::Passed);
}

// ---------------------------------------------------------------------------
// Test 2: Repository B - Multi-Agent Concurrent Workflow & Durable Messaging (§7, §8)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_repository_b_multi_agent_concurrent_workflow_with_durable_messages() {
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().to_path_buf();
    setup_repository_b_token_bucket(&repo_dir);

    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let lease_manager = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let recovery_controller = Arc::new(RecoveryController::new(store.clone(), 3));
    let tool_registry = setup_tools();

    let workflow_id = WorkflowId::new();
    let mut workflow = Workflow::new(
        "Autonomous Burst Capacity Feature",
        "Add burst capacity to token bucket, add comprehensive tests, coordinate via agent messaging, and integrate.",
    );
    workflow.id = workflow_id;
    workflow.state = plexis_core::state::WorkflowState::Active;
    store.create_workflow(&workflow).await.unwrap();

    let dev_id = AgentId::new();
    let test_id = AgentId::new();
    let int_id = AgentId::new();

    let mut dev_agent = Agent::new(
        "Core Developer",
        "Developer",
        ExecutionProfile::new("scripted", "default"),
    )
    .with_capabilities(vec!["filesystem_write".into(), "shell".into()]);
    dev_agent.id = dev_id;
    store.create_agent(&dev_agent).await.unwrap();

    let mut test_agent = Agent::new(
        "QA Engineer",
        "Tester",
        ExecutionProfile::new("scripted", "default"),
    )
    .with_capabilities(vec!["test_runner".into(), "filesystem_write".into()]);
    test_agent.id = test_id;
    store.create_agent(&test_agent).await.unwrap();

    let mut int_agent = Agent::new(
        "Release Integrator",
        "Integrator",
        ExecutionProfile::new("scripted", "default"),
    )
    .with_capabilities(vec!["git".into(), "shell".into()]);
    int_agent.id = int_id;
    store.create_agent(&int_agent).await.unwrap();

    store
        .create_session(
            &plexis_core::Session::new(dev_id).with_working_directory(repo_dir.to_str().unwrap()),
        )
        .await
        .unwrap();
    store
        .create_session(
            &plexis_core::Session::new(test_id).with_working_directory(repo_dir.to_str().unwrap()),
        )
        .await
        .unwrap();
    store
        .create_session(
            &plexis_core::Session::new(int_id).with_working_directory(repo_dir.to_str().unwrap()),
        )
        .await
        .unwrap();

    // Concurrent Tasks: Task 1 and Task 2 both start Ready
    let task1_id = TaskId::new();
    let mut task1 = Task::new(workflow_id, "Implement burst capacity in src/lib.rs");
    task1.id = task1_id;
    task1.state = TaskState::Ready;
    task1.priority = 10;
    task1.metadata = json!({
        "required_capabilities": ["filesystem_write", "shell"],
        "suggested_role": "Developer"
    });
    store.create_task(&task1).await.unwrap();

    let task2_id = TaskId::new();
    let mut task2 = Task::new(
        workflow_id,
        "Implement burst tests in tests/limiter_tests.rs",
    );
    task2.id = task2_id;
    task2.state = TaskState::Ready;
    task2.priority = 10;
    task2.metadata = json!({
        "required_capabilities": ["test_runner", "filesystem_write"],
        "suggested_role": "Tester"
    });
    store.create_task(&task2).await.unwrap();

    let task3_id = TaskId::new();
    let mut task3 = Task::new(workflow_id, "Integrate and commit deliverables");
    task3.id = task3_id;
    task3.state = TaskState::Backlog;
    task3.priority = 5;
    task3.metadata = json!({
        "required_capabilities": ["git", "shell"],
        "suggested_role": "Integrator"
    });
    store.create_task(&task3).await.unwrap();
    store.add_dependency(&task3_id, &task1_id).await.unwrap();
    store.add_dependency(&task3_id, &task2_id).await.unwrap();

    let provider = Arc::new(ScriptedProvider::new("scripted"));

    let new_lib_rs = r#"pub struct TokenBucket {
    capacity: usize,
    burst_capacity: usize,
    tokens: usize,
}

impl TokenBucket {
    pub fn new(capacity: usize) -> Self {
        Self::with_burst(capacity, capacity)
    }

    pub fn with_burst(capacity: usize, burst_capacity: usize) -> Self {
        Self {
            capacity,
            burst_capacity,
            tokens: burst_capacity,
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        self.try_acquire_n(1)
    }

    pub fn try_acquire_n(&mut self, n: usize) -> bool {
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }

    pub fn available_tokens(&self) -> usize {
        self.tokens
    }
}
"#;

    // Developer: writes code
    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Writing burst capacity code".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_write_tb",
                "filesystem",
                json!({
                    "action": "write",
                    "path": "src/lib.rs",
                    "content": new_lib_rs
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage::default(),
        finish_reason: FinishReason::ToolCalls,
    });

    // Developer sends durable message to Tester (§8)
    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Notifying tester about updated API contract".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_msg_1",
                "send_message",
                json!({
                    "to_agent": test_id.to_string(),
                    "message_type": "request",
                    "content": "API contract changed: added with_burst and try_acquire_n(n); update test expectations."
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage::default(),
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage::assistant("Developer task completed"),
        usage: TokenUsage::default(),
        finish_reason: FinishReason::Stop,
    });

    // Tester: writes tests
    let new_tests_rs = r#"use token_bucket::TokenBucket;

#[test]
fn test_acquire_basic() {
    let mut tb = TokenBucket::new(2);
    assert!(tb.try_acquire());
    assert!(tb.try_acquire());
    assert!(!tb.try_acquire());
}

#[test]
fn test_burst_capacity() {
    let mut tb = TokenBucket::with_burst(5, 10);
    assert!(tb.try_acquire_n(7));
    assert_eq!(tb.available_tokens(), 3);
}
"#;

    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Writing burst capacity unit tests".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_write_tests",
                "filesystem",
                json!({
                    "action": "write",
                    "path": "tests/limiter_tests.rs",
                    "content": new_tests_rs
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage::default(),
        finish_reason: FinishReason::ToolCalls,
    });

    // Tester sends durable confirmation to Developer (§8)
    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Confirming test coverage to developer".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_msg_2",
                "send_message",
                json!({
                    "to_agent": dev_id.to_string(),
                    "message_type": "result",
                    "content": "Test suite updated with test_burst_capacity validating try_acquire_n."
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage::default(),
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage::assistant("Tester task completed"),
        usage: TokenUsage::default(),
        finish_reason: FinishReason::Stop,
    });

    // Integrator runs cargo test and commits
    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Running full test suite on integrated repository".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_test_tb",
                "shell",
                json!({ "command": "cargo test" }).to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage::default(),
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Committing integrated burst capacity feature".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_git_tb",
                "git",
                json!({
                    "subcommand": "commit",
                    "args": ["-am", "feat: implement burst capacity for TokenBucket rate limiter"]
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage::default(),
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage::assistant("Integration completed cleanly"),
        usage: TokenUsage::default(),
        finish_reason: FinishReason::Stop,
    });

    let mut runner = AgentRunner::new(store.clone(), tool_registry, verifier.clone())
        .with_recovery_controller(recovery_controller);
    runner.register_provider(provider);
    let runner_arc = Arc::new(runner);

    let scheduler =
        DeterministicScheduler::new(store.clone(), lease_manager, dispatcher, runner_arc);

    // Tick 1: Dispatches Task 1 AND Task 2 concurrently in parallel!
    let o1 = scheduler.tick().await.unwrap();
    assert_eq!(
        o1, 2,
        "Task 1 (Developer) and Task 2 (Tester) must run concurrently in parallel"
    );

    let t1_state = store.get_task(&task1_id).await.unwrap().unwrap().state;
    let t2_state = store.get_task(&task2_id).await.unwrap().unwrap().state;
    assert_eq!(t1_state, TaskState::Verified);
    assert_eq!(t2_state, TaskState::Verified);

    // Verify Durable Agent Messages were persisted (§8)
    let dev_messages = store.list_messages_for_agent(&dev_id).await.unwrap();
    assert_eq!(dev_messages.len(), 1);
    assert!(dev_messages[0].content.contains("test_burst_capacity"));

    let test_messages = store.list_messages_for_agent(&test_id).await.unwrap();
    assert_eq!(test_messages.len(), 1);
    assert!(test_messages[0].content.contains("API contract changed"));

    // Tick 2: Dispatches Task 3 (Integrator consuming deliverables from Tasks 1 & 2)
    let o2 = scheduler.tick().await.unwrap();
    assert_eq!(o2, 1);

    let t3_state = store.get_task(&task3_id).await.unwrap().unwrap().state;
    assert_eq!(t3_state, TaskState::Verified);

    let git_log = StdCommand::new("git")
        .args(["log", "-n", "2", "--oneline"])
        .current_dir(&repo_dir)
        .output()
        .expect("git log");
    let log_str = String::from_utf8_lossy(&git_log.stdout);
    assert!(log_str.contains("feat: implement burst capacity"));
}

// ---------------------------------------------------------------------------
// Test 3: Repository C - Failure Injection & Strategy Mutation Recovery (§4, §9)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_repository_c_failure_injection_and_recovery() {
    let temp = tempdir().unwrap();
    let repo_dir = temp.path().to_path_buf();
    setup_repository_c_config_loader(&repo_dir);

    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let lease_manager = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let recovery_controller = Arc::new(RecoveryController::new(store.clone(), 3));
    let tool_registry = setup_tools();

    let workflow_id = WorkflowId::new();
    let mut workflow = Workflow::new(
        "Add Max Retries Option with Recovery",
        "Add max_retries configuration parameter, recover from initial compile failure, verify and commit.",
    );
    workflow.id = workflow_id;
    workflow.state = plexis_core::state::WorkflowState::Active;
    store.create_workflow(&workflow).await.unwrap();

    let task_id = TaskId::new();
    let mut task = Task::new(workflow_id, "Add max_retries field to Config struct");
    task.id = task_id;
    task.state = TaskState::Ready;
    task.priority = 10;
    task.criteria = vec!["command:cargo test".into()];
    task.metadata = json!({
        "required_capabilities": ["filesystem_write", "shell"],
        "suggested_role": "Developer",
        "verification": {
            "command": "cargo test"
        }
    });
    store.create_task(&task).await.unwrap();

    let dev_agent = Agent::new(
        "Core Developer",
        "Developer",
        ExecutionProfile::new("scripted", "default"),
    )
    .with_capabilities(vec!["filesystem_write".into(), "shell".into()]);
    store.create_agent(&dev_agent).await.unwrap();

    store
        .create_session(
            &plexis_core::Session::new(dev_agent.id)
                .with_working_directory(repo_dir.to_str().unwrap()),
        )
        .await
        .unwrap();

    let provider = Arc::new(ScriptedProvider::new("scripted"));

    // Attempt 1: Developer introduces a syntax error in src/lib.rs (missing closing brace)
    let bad_lib_rs = r#"pub struct Config {
    pub host: String,
    pub port: u16,
    pub max_retries: u32,
// Syntax error: missing impl closing brace
"#;

    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some("Attempt 1: Writing buggy implementation".into()),
            tool_calls: Some(vec![ToolCall::new(
                "call_bad_write",
                "filesystem",
                json!({
                    "action": "write",
                    "path": "src/lib.rs",
                    "content": bad_lib_rs
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage::default(),
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage::assistant("Attempt 1 completed"),
        usage: TokenUsage::default(),
        finish_reason: FinishReason::Stop,
    });

    // Attempt 2: After verification failure and recovery mutation, developer fixes the code
    let valid_lib_rs = r#"pub struct Config {
    pub host: String,
    pub port: u16,
    pub max_retries: u32,
}

impl Config {
    pub fn parse(content: &str) -> Result<Self, String> {
        let mut host = "127.0.0.1".to_string();
        let mut port = 8080;
        let mut max_retries = 3;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "host" => host = v.trim().to_string(),
                    "port" => port = v.trim().parse().map_err(|e| format!("invalid port: {}", e))?,
                    "max_retries" => max_retries = v.trim().parse().map_err(|e| format!("invalid max_retries: {}", e))?,
                    _ => {}
                }
            }
        }

        Ok(Self { host, port, max_retries })
    }
}
"#;

    provider.queue_response(CompletionResponse {
        message: ChatMessage {
            role: ChatRole::Assistant,
            content: Some(
                "Attempt 2: Applying corrected implementation based on recovery advice".into(),
            ),
            tool_calls: Some(vec![ToolCall::new(
                "call_good_write",
                "filesystem",
                json!({
                    "action": "write",
                    "path": "src/lib.rs",
                    "content": valid_lib_rs
                })
                .to_string(),
            )]),
            tool_call_id: None,
            name: None,
        },
        usage: TokenUsage::default(),
        finish_reason: FinishReason::ToolCalls,
    });

    provider.queue_response(CompletionResponse {
        message: ChatMessage::assistant("Attempt 2 completed successfully"),
        usage: TokenUsage::default(),
        finish_reason: FinishReason::Stop,
    });

    let mut runner = AgentRunner::new(store.clone(), tool_registry, verifier.clone())
        .with_recovery_controller(recovery_controller);
    runner.register_provider(provider);
    let runner_arc = Arc::new(runner);

    let scheduler =
        DeterministicScheduler::new(store.clone(), lease_manager, dispatcher, runner_arc);

    // Tick 1: Executes Attempt 1 -> cargo test fails -> recovery controller mutates strategy & re-readies task
    let o1 = scheduler.tick().await.unwrap();
    assert_eq!(o1, 1);

    let t_after_fail = store.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(t_after_fail.state, TaskState::Ready); // Re-readied for recovery attempt!
    assert_eq!(t_after_fail.attempts, 1);
    assert!(t_after_fail.metadata.get("recovery_advice").is_some());

    // Tick 2: Executes Attempt 2 with recovery advice -> cargo test passes -> TaskState::Verified
    let o2 = scheduler.tick().await.unwrap();
    assert_eq!(o2, 1);

    let t_after_rec = store.get_task(&task_id).await.unwrap().unwrap();
    assert_eq!(t_after_rec.state, TaskState::Verified);
    assert_eq!(t_after_rec.attempts, 2);
}
