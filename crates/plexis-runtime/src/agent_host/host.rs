//! Local Agent Host
//!
//! Supervises external autonomous coding agent OS processes across a physical
//! OS process boundary with process group isolation, timeout enforcement,
//! output streaming, and cancellation signaling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use plexis_core::ids::ExecutionId;
use plexis_core::protocol::{ExecutionEvent, ExecutionRequest, ExecutionResult};
use serde::{Deserialize, Serialize};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{timeout, Duration};

use super::security::{EnvironmentScrubber, WorkspaceValidator};
use crate::error::RuntimeError;

/// Lifecycle state of a supervised external agent process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessState {
    Spawning,
    Running {
        pid: u32,
        pgid: u32,
        started_at: DateTime<Utc>,
    },
    Completed {
        exit_code: i32,
    },
    Failed {
        reason: String,
        exit_code: Option<i32>,
    },
    Cancelled,
    TimedOut,
}

/// Pluggable line parser translating raw CLI lines into structured Plexis ExecutionEvents.
pub trait OutputParser: Send + Sync {
    fn parse_line(&self, execution_id: ExecutionId, line: &str) -> Option<ExecutionEvent>;
}

/// Raw execution result of a spawned external process command.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub cancelled: bool,
}

/// Metadata tracked for an active running external agent process.
#[derive(Debug, Clone)]
pub struct ActiveProcess {
    pub execution_id: ExecutionId,
    pub pid: u32,
    pub pgid: u32,
    pub state: ProcessState,
    pub started_at: DateTime<Utc>,
}

/// Subsystem that spawns and supervises external coding-agent OS processes.
pub struct LocalAgentHost {
    executable_path: PathBuf,
    active_processes: Arc<RwLock<HashMap<ExecutionId, ActiveProcess>>>,
}

impl LocalAgentHost {
    /// Creates a new LocalAgentHost targeting a specific external agent executable.
    pub fn new(executable_path: PathBuf) -> Self {
        let abs_path = if let Ok(canonical) = executable_path.canonicalize() {
            canonical
        } else {
            executable_path
        };
        Self {
            executable_path: abs_path,
            active_processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Auto-detects the default `plexis-fake-agent` binary in standard build locations.
    pub fn with_default_binary() -> Self {
        let binary_name = if cfg!(windows) {
            "plexis-fake-agent.exe"
        } else {
            "plexis-fake-agent"
        };

        let candidate_dirs = [
            // Current exe dir
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.to_path_buf())),
            // target/debug
            Some(PathBuf::from("target/debug")),
            // ../target/debug
            Some(PathBuf::from("../target/debug")),
            // ../../target/debug
            Some(PathBuf::from("../../target/debug")),
        ];

        for cand_dir in candidate_dirs.into_iter().flatten() {
            let cand = cand_dir.join(binary_name);
            if cand.exists() {
                return Self::new(cand);
            }

            // Check deps/ directory for cargo test artifacts (e.g. deps/plexis_fake_agent-*)
            let deps_dir = cand_dir.join("deps");
            if deps_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&deps_dir) {
                    let mut matches = Vec::new();
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            let is_match = if cfg!(windows) {
                                file_name.starts_with("plexis_fake_agent-")
                                    && file_name.ends_with(".exe")
                            } else {
                                file_name.starts_with("plexis_fake_agent-")
                                    && !file_name.contains('.')
                            };
                            if is_match && path.is_file() {
                                matches.push(path);
                            }
                        }
                    }
                    matches.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
                    if let Some(best) = matches.pop() {
                        return Self::new(best);
                    }
                }
            }
        }

        // Check if we are running in a cargo workspace and need to build the test agent
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok().map(PathBuf::from);
        let mut cur = manifest_dir.or_else(|| std::env::current_dir().ok());
        let mut root_dir = None;
        while let Some(dir) = cur {
            if dir.join("Cargo.toml").is_file() {
                if let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) {
                    if content.contains("[workspace]") {
                        root_dir = Some(dir);
                        break;
                    }
                }
            }
            cur = dir.parent().map(|p| p.to_path_buf());
        }

        if let Some(root) = root_dir {
            let target_bin = root.join("target").join("debug").join(binary_name);
            if !target_bin.exists() {
                let _ = std::process::Command::new("cargo")
                    .args(["build", "-p", "plexis-fake-agent"])
                    .current_dir(&root)
                    .output();
            }
            if target_bin.exists() {
                return Self::new(target_bin);
            }
        }

        // Fallback to name on PATH
        Self::new(PathBuf::from(binary_name))
    }

    pub fn executable_path(&self) -> &Path {
        &self.executable_path
    }

    /// Queries the current lifecycle state of an execution process.
    pub async fn get_process_state(&self, execution_id: &ExecutionId) -> Option<ProcessState> {
        self.active_processes
            .read()
            .await
            .get(execution_id)
            .map(|p| p.state.clone())
    }

    /// Lists all active external agent processes currently supervised.
    pub async fn list_active_processes(&self) -> Vec<ActiveProcess> {
        self.active_processes
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Cancels a running external agent process by terminating its entire process group.
    pub async fn cancel_execution(&self, execution_id: &ExecutionId) -> Result<(), RuntimeError> {
        let active = {
            let mut procs = self.active_processes.write().await;
            if let Some(proc_info) = procs.get_mut(execution_id) {
                proc_info.state = ProcessState::Cancelled;
                Some(proc_info.clone())
            } else {
                None
            }
        };

        if let Some(proc_info) = active {
            #[cfg(unix)]
            {
                let pgid = -(proc_info.pgid as i32);
                tracing::info!(
                    execution_id = %execution_id,
                    pgid = proc_info.pgid,
                    "Sending SIGTERM to external agent process group"
                );

                // 1. Send SIGTERM to entire process group
                unsafe {
                    libc::kill(pgid, libc::SIGTERM);
                }

                // 2. Wait grace period
                tokio::time::sleep(Duration::from_millis(500)).await;

                // 3. Send SIGKILL to ensure no surviving descendants
                unsafe {
                    libc::kill(pgid, libc::SIGKILL);
                }
            }

            #[cfg(not(unix))]
            {
                tracing::warn!("Process group kill not supported on non-unix platform");
            }

            Ok(())
        } else {
            Err(RuntimeError::InvalidCommand(format!(
                "No active process found for execution {}",
                execution_id
            )))
        }
    }

    /// Spawns an external coding agent process, pipes request, streams events, and enforces timeout.
    pub async fn spawn_execution(
        &self,
        request: ExecutionRequest,
        event_sender: Option<mpsc::Sender<ExecutionEvent>>,
    ) -> Result<ExecutionResult, RuntimeError> {
        let start_time = Instant::now();
        let exec_id = request.execution_id;

        // 1. Security check: Validate and canonicalize workspace path
        let validated_workspace =
            WorkspaceValidator::validate_and_canonicalize(&request.workspace_path)?;

        // 2. Security check: Scrub environment variables
        let scrubbed_env = EnvironmentScrubber::prepare_child_environment(
            &request.environment,
            &exec_id.to_string(),
            &validated_workspace,
        );

        // 3. Verify executable exists
        if !self.executable_path.exists() && self.executable_path.is_absolute() {
            return Err(RuntimeError::InvalidCommand(format!(
                "Agent executable not found at: {}",
                self.executable_path.display()
            )));
        }

        // 4. Configure process command with isolated process group
        let mut cmd = Command::new(&self.executable_path);
        cmd.current_dir(&validated_workspace);
        cmd.env_clear();
        cmd.envs(scrubbed_env);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(unix)]
        {
            // Sets process group ID to the child PID, creating an isolated process group
            cmd.process_group(0);
        }

        let mut child = cmd.spawn().map_err(|e| {
            RuntimeError::InvalidCommand(format!(
                "Failed to spawn external agent executable {}: {}",
                self.executable_path.display(),
                e
            ))
        })?;

        let pid = child.id().unwrap_or(0);
        let pgid = pid; // Under process_group(0), pgid == pid

        let now = Utc::now();
        {
            let mut procs = self.active_processes.write().await;
            procs.insert(
                exec_id,
                ActiveProcess {
                    execution_id: exec_id,
                    pid,
                    pgid,
                    state: ProcessState::Running {
                        pid,
                        pgid,
                        started_at: now,
                    },
                    started_at: now,
                },
            );
        }

        // 5. Send ExecutionRequest JSON over stdin
        if let Some(mut stdin) = child.stdin.take() {
            let req_json = serde_json::to_string(&request).map_err(|e| {
                RuntimeError::InvalidCommand(format!("Failed to serialize ExecutionRequest: {}", e))
            })?;
            stdin.write_all(req_json.as_bytes()).await.map_err(|e| {
                RuntimeError::InvalidCommand(format!("Failed to write to stdin: {}", e))
            })?;
            stdin.write_all(b"\n").await.map_err(|e| {
                RuntimeError::InvalidCommand(format!("Failed to write newline: {}", e))
            })?;
            stdin.flush().await.map_err(|e| {
                RuntimeError::InvalidCommand(format!("Failed to flush stdin: {}", e))
            })?;
            drop(stdin); // Close stdin so agent knows request stream is complete
        }

        // 6. Setup streaming loops for stdout and stderr
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RuntimeError::Execution("Failed to capture stdout".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| RuntimeError::Execution("Failed to capture stderr".into()))?;

        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let event_tx_out = event_sender.clone();
        let event_tx_err = event_sender.clone();

        let (result_tx, mut result_rx) = mpsc::channel::<ExecutionResult>(1);

        // Task: Read stdout line by line
        let stdout_task = tokio::spawn(async move {
            let mut lines = stdout_reader.lines();
            let mut captured_stdout = Vec::new();
            let mut parsed_result: Option<ExecutionResult> = None;

            while let Ok(Some(line)) = lines.next_line().await {
                captured_stdout.push(line.clone());

                // Check if line is a serialized ExecutionEvent
                if let Ok(event) = serde_json::from_str::<ExecutionEvent>(&line) {
                    if let Some(ref tx) = event_tx_out {
                        let _ = tx.send(event).await;
                    }
                    continue;
                }

                // Check if line is a serialized ExecutionResult
                if let Ok(res) = serde_json::from_str::<ExecutionResult>(&line) {
                    parsed_result = Some(res);
                    continue;
                }

                // Plain text fallback
                if let Some(ref tx) = event_tx_out {
                    let ev = ExecutionEvent::stdout(exec_id, line);
                    let _ = tx.send(ev).await;
                }
            }

            if let Some(res) = parsed_result {
                let _ = result_tx.send(res).await;
            }

            captured_stdout.join("\n")
        });

        // Task: Read stderr line by line
        let stderr_task = tokio::spawn(async move {
            let mut lines = stderr_reader.lines();
            let mut captured_stderr = Vec::new();

            while let Ok(Some(line)) = lines.next_line().await {
                captured_stderr.push(line.clone());
                if let Some(ref tx) = event_tx_err {
                    let ev = ExecutionEvent::stderr(exec_id, line);
                    let _ = tx.send(ev).await;
                }
            }

            captured_stderr.join("\n")
        });

        // 7. Enforce timeout during child execution
        let timeout_duration = Duration::from_secs(request.timeout_secs.max(1));
        let wait_result = timeout(timeout_duration, child.wait()).await;

        let exit_status = match wait_result {
            Ok(status_res) => match status_res {
                Ok(status) => status,
                Err(e) => {
                    self.cleanup_process(exec_id, pgid).await;
                    return Err(RuntimeError::Execution(format!(
                        "Process wait failed for execution {}: {}",
                        exec_id, e
                    )));
                }
            },
            Err(_) => {
                // Timeout exceeded!
                tracing::warn!(
                    execution_id = %exec_id,
                    timeout_secs = request.timeout_secs,
                    "External agent execution timed out, killing process group"
                );

                self.cleanup_process(exec_id, pgid).await;

                if let Some(ref tx) = event_sender {
                    let _ = tx
                        .send(ExecutionEvent::failed(
                            exec_id,
                            format!("Execution timed out after {}s", request.timeout_secs),
                            Some(124),
                        ))
                        .await;
                }

                let duration = start_time.elapsed().as_millis() as u64;
                let fail_res = ExecutionResult::failure(
                    exec_id,
                    124,
                    format!("Execution timed out after {}s", request.timeout_secs),
                    duration,
                )
                .with_pid(pid);

                {
                    let mut procs = self.active_processes.write().await;
                    if let Some(p) = procs.get_mut(&exec_id) {
                        p.state = ProcessState::TimedOut;
                    }
                }

                return Ok(fail_res);
            }
        };

        // 8. Reaping and final outcome resolution
        let raw_stdout = stdout_task.await.unwrap_or_default();
        let raw_stderr = stderr_task.await.unwrap_or_default();

        let code = exit_status.code().unwrap_or(1);
        let duration = start_time.elapsed().as_millis() as u64;

        // Retrieve structured result if child outputted one, or synthesize
        let final_result = if let Ok(parsed) = result_rx.try_recv() {
            parsed
                .with_raw_output(Some(raw_stdout), Some(raw_stderr))
                .with_pid(pid)
        } else if code == 0 {
            ExecutionResult::success(
                exec_id,
                "External agent execution completed successfully",
                vec![],
                None,
                duration,
            )
            .with_raw_output(Some(raw_stdout), Some(raw_stderr))
            .with_pid(pid)
        } else {
            let reason = if !raw_stderr.trim().is_empty() {
                raw_stderr.trim().to_string()
            } else {
                format!("Process exited with status code {}", code)
            };
            ExecutionResult::failure(exec_id, code, reason, duration)
                .with_raw_output(Some(raw_stdout), Some(raw_stderr))
                .with_pid(pid)
        };

        // Update active process registry
        {
            let mut procs = self.active_processes.write().await;
            if let Some(p) = procs.get_mut(&exec_id) {
                if p.state == ProcessState::Cancelled {
                    // Retain cancelled state
                } else if code == 0 {
                    p.state = ProcessState::Completed { exit_code: 0 };
                } else {
                    p.state = ProcessState::Failed {
                        reason: final_result
                            .failure_reason
                            .clone()
                            .unwrap_or_else(|| format!("Exit code {}", code)),
                        exit_code: Some(code),
                    };
                }
            }
        }

        Ok(final_result)
    }

    /// Spawns an external command with arguments, process-group isolation,
    /// environment scrubbing, streaming output translation, and timeout/cancellation supervision.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_command_execution(
        &self,
        execution_id: ExecutionId,
        command_path: &Path,
        args: &[String],
        workspace_path: &Path,
        custom_env: &HashMap<String, String>,
        timeout_secs: u64,
        event_sender: Option<mpsc::Sender<ExecutionEvent>>,
        output_parser: Option<Arc<dyn OutputParser>>,
    ) -> Result<CommandOutput, RuntimeError> {
        let start_time = Instant::now();

        // 1. Validate and canonicalize workspace path
        let validated_workspace = WorkspaceValidator::validate_and_canonicalize(workspace_path)?;

        // 2. Prepare scrubbed environment with permitted overrides
        let scrubbed_env = EnvironmentScrubber::prepare_child_environment(
            custom_env,
            &execution_id.to_string(),
            &validated_workspace,
        );

        // 3. Verify command executable exists
        if !command_path.exists() && command_path.is_absolute() {
            return Err(RuntimeError::InvalidCommand(format!(
                "Command executable not found at: {}",
                command_path.display()
            )));
        }

        // 4. Configure process command with isolated process group
        let mut cmd = Command::new(command_path);
        cmd.args(args);
        cmd.current_dir(&validated_workspace);
        cmd.env_clear();
        cmd.envs(scrubbed_env);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(unix)]
        {
            cmd.process_group(0);
        }

        let mut child = cmd.spawn().map_err(|e| {
            RuntimeError::InvalidCommand(format!(
                "Failed to spawn command {}: {}",
                command_path.display(),
                e
            ))
        })?;

        let pid = child.id().unwrap_or(0);
        let pgid = pid;

        tracing::info!(
            "Spawned external command {} (pid: {}, args: {:?})",
            command_path.display(),
            pid,
            args
        );

        let now = Utc::now();
        {
            let mut procs = self.active_processes.write().await;
            procs.insert(
                execution_id,
                ActiveProcess {
                    execution_id,
                    pid,
                    pgid,
                    state: ProcessState::Running {
                        pid,
                        pgid,
                        started_at: now,
                    },
                    started_at: now,
                },
            );
        }

        // Notify that process has started
        if let Some(ref tx) = event_sender {
            let _ = tx.send(ExecutionEvent::started(execution_id, pid)).await;
        }

        // 5. Setup streaming tasks for stdout and stderr
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| RuntimeError::Execution("Failed to capture stdout".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| RuntimeError::Execution("Failed to capture stderr".into()))?;

        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let event_tx_out = event_sender.clone();
        let parser_clone = output_parser.clone();

        let stdout_task = tokio::spawn(async move {
            let mut lines = stdout_reader.lines();
            let mut captured = Vec::new();

            while let Ok(Some(line)) = lines.next_line().await {
                captured.push(line.clone());

                if let Some(ref tx) = event_tx_out {
                    if let Some(ref parser) = parser_clone {
                        if let Some(ev) = parser.parse_line(execution_id, &line) {
                            let _ = tx.send(ev).await;
                            continue;
                        }
                    }
                    let ev = ExecutionEvent::stdout(execution_id, line);
                    let _ = tx.send(ev).await;
                }
            }

            captured.join("\n")
        });

        let event_tx_err = event_sender.clone();
        let stderr_task = tokio::spawn(async move {
            let mut lines = stderr_reader.lines();
            let mut captured = Vec::new();

            while let Ok(Some(line)) = lines.next_line().await {
                captured.push(line.clone());
                if let Some(ref tx) = event_tx_err {
                    let ev = ExecutionEvent::stderr(execution_id, line);
                    let _ = tx.send(ev).await;
                }
            }

            captured.join("\n")
        });

        // 6. Enforce timeout
        let timeout_duration = Duration::from_secs(timeout_secs.max(1));
        let wait_result = timeout(timeout_duration, child.wait()).await;

        let (exit_code, timed_out) = match wait_result {
            Ok(status_res) => match status_res {
                Ok(status) => (status.code().unwrap_or(1), false),
                Err(e) => {
                    self.cleanup_process(execution_id, pgid).await;
                    return Err(RuntimeError::Execution(format!(
                        "Process wait failed for execution {}: {}",
                        execution_id, e
                    )));
                }
            },
            Err(_) => {
                tracing::warn!(
                    execution_id = %execution_id,
                    timeout_secs = timeout_secs,
                    "Command execution timed out, terminating process group"
                );
                self.cleanup_process(execution_id, pgid).await;

                if let Some(ref tx) = event_sender {
                    let _ = tx
                        .send(ExecutionEvent::failed(
                            execution_id,
                            format!("Execution timed out after {}s", timeout_secs),
                            Some(124),
                        ))
                        .await;
                }

                {
                    let mut procs = self.active_processes.write().await;
                    if let Some(p) = procs.get_mut(&execution_id) {
                        p.state = ProcessState::TimedOut;
                    }
                }

                (124, true)
            }
        };

        let raw_stdout = stdout_task.await.unwrap_or_default();
        let raw_stderr = stderr_task.await.unwrap_or_default();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        let is_cancelled = {
            let procs = self.active_processes.read().await;
            procs
                .get(&execution_id)
                .map(|p| p.state == ProcessState::Cancelled)
                .unwrap_or(false)
        };

        if !timed_out && !is_cancelled {
            let mut procs = self.active_processes.write().await;
            if let Some(p) = procs.get_mut(&execution_id) {
                if exit_code == 0 {
                    p.state = ProcessState::Completed { exit_code: 0 };
                } else {
                    p.state = ProcessState::Failed {
                        reason: format!("Exit code {}", exit_code),
                        exit_code: Some(exit_code),
                    };
                }
            }
        }

        Ok(CommandOutput {
            exit_code,
            stdout: raw_stdout,
            stderr: raw_stderr,
            duration_ms,
            timed_out,
            cancelled: is_cancelled,
        })
    }

    /// Terminates the process group cleanly, escalating from SIGTERM to SIGKILL.
    async fn cleanup_process(&self, _execution_id: ExecutionId, pgid: u32) {
        #[cfg(unix)]
        {
            let neg_pgid = -(pgid as i32);
            unsafe {
                libc::kill(neg_pgid, libc::SIGTERM);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            unsafe {
                libc::kill(neg_pgid, libc::SIGKILL);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plexis_core::ids::AgentId;
    use plexis_core::protocol::ExecutionEventType;
    use tempfile::tempdir;

    fn get_test_binary() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest_dir.parent().unwrap().parent().unwrap();
        let binary_path = root.join("target/debug/plexis-fake-agent");
        if !binary_path.exists() {
            let _ = std::process::Command::new("cargo")
                .args(["build", "-p", "plexis-fake-agent"])
                .current_dir(root)
                .output();
        }
        if binary_path.exists() {
            binary_path
        } else {
            PathBuf::from("plexis-fake-agent")
        }
    }

    #[tokio::test]
    async fn test_local_agent_host_spawn_and_complete() {
        let bin = get_test_binary();
        if !bin.exists() {
            eprintln!("Skipping test: binary not found at {:?}", bin);
            return;
        }

        let host = LocalAgentHost::new(bin);
        let dir = tempdir().expect("tempdir");

        // Initialize a minimal Rust crate in tempdir
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test_pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn compute(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .unwrap();

        let exec_id = ExecutionId::new();
        let agent_id = AgentId::new();
        let req = ExecutionRequest::new(
            exec_id,
            agent_id,
            "Developer",
            "Fix arithmetic bug in modulo",
            dir.path().to_path_buf(),
        )
        .with_timeout_secs(30);

        let (tx, mut rx) = mpsc::channel(50);

        let res = host.spawn_execution(req, Some(tx)).await.expect("spawn");
        assert!(res.success, "Execution failed: {:?}", res.failure_reason);
        assert_eq!(res.exit_code, 0);

        // Check events received
        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }
        assert!(!events.is_empty());
        assert!(events
            .iter()
            .any(|e| matches!(e.event, ExecutionEventType::Started { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.event, ExecutionEventType::Completed)));
    }

    #[tokio::test]
    async fn test_local_agent_host_timeout() {
        let bin = get_test_binary();
        if !bin.exists() {
            return;
        }

        let host = LocalAgentHost::new(bin);
        let dir = tempdir().expect("tempdir");

        let exec_id = ExecutionId::new();
        let agent_id = AgentId::new();
        let req = ExecutionRequest::new(
            exec_id,
            agent_id,
            "Developer",
            "Test hanging agent",
            dir.path().to_path_buf(),
        )
        .with_failure_mode("hang")
        .with_timeout_secs(2);

        let res = host.spawn_execution(req, None).await.expect("spawn");
        assert!(!res.success);
        assert!(
            res.failure_reason
                .as_deref()
                .unwrap_or("")
                .contains("timed out"),
            "Expected timeout failure, got: {:?}",
            res.failure_reason
        );
    }

    #[tokio::test]
    async fn test_local_agent_host_cancellation() {
        let bin = get_test_binary();
        if !bin.exists() {
            return;
        }

        let host = Arc::new(LocalAgentHost::new(bin));
        let dir = tempdir().expect("tempdir");

        let exec_id = ExecutionId::new();
        let agent_id = AgentId::new();
        let req = ExecutionRequest::new(
            exec_id,
            agent_id,
            "Developer",
            "Test long agent cancellation",
            dir.path().to_path_buf(),
        )
        .with_delay_ms(5000)
        .with_timeout_secs(30);

        let host_clone = host.clone();
        let handle = tokio::spawn(async move { host_clone.spawn_execution(req, None).await });

        // Wait a bit for process to start
        tokio::time::sleep(Duration::from_millis(300)).await;

        let cancel_res = host.cancel_execution(&exec_id).await;
        assert!(cancel_res.is_ok(), "Cancel failed: {:?}", cancel_res);

        let exec_res = handle.await.expect("join").expect("exec_res");
        assert!(!exec_res.success);
    }

    #[tokio::test]
    async fn test_local_agent_host_crash_handling() {
        let bin = get_test_binary();
        if !bin.exists() {
            return;
        }

        let host = LocalAgentHost::new(bin);
        let dir = tempdir().expect("tempdir");

        let exec_id = ExecutionId::new();
        let agent_id = AgentId::new();
        let req = ExecutionRequest::new(
            exec_id,
            agent_id,
            "Developer",
            "Test crashing agent",
            dir.path().to_path_buf(),
        )
        .with_failure_mode("crash")
        .with_timeout_secs(10);

        let res = host.spawn_execution(req, None).await.expect("spawn");
        assert!(!res.success);
        assert_eq!(res.exit_code, 139);
    }
}
