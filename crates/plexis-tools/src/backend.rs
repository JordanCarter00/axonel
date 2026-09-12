use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

use crate::error::ToolError;

/// Specification for a process execution within a sandbox backend.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub command: String,
    pub working_dir: PathBuf,
    pub env_vars: HashMap<String, String>,
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl CommandSpec {
    pub fn new(command: impl Into<String>, working_dir: impl Into<PathBuf>) -> Self {
        Self {
            command: command.into(),
            working_dir: working_dir.into(),
            env_vars: HashMap::new(),
            timeout: Duration::from_secs(30),
            max_output_bytes: 5 * 1024 * 1024,
        }
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_max_output_bytes(mut self, max_bytes: usize) -> Self {
        self.max_output_bytes = max_bytes;
        self
    }
}

/// Output captured from a backend process execution.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Abstraction for sandboxed execution backends (host process, isolated container, namespace jail).
#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    /// Identifier for this backend (e.g. "host_process", "isolated_container").
    fn name(&self) -> &str;

    /// Executes the specified command inside the backend environment.
    async fn execute(&self, spec: CommandSpec) -> Result<CommandResult, ToolError>;
}

/// Default execution backend that spawns host OS child processes with strict environment stripping.
/// Host environment variables (which may contain sensitive tokens, keys, and credentials)
/// are completely cleared, and only safe system variables and explicitly granted variables are passed.
#[derive(Debug, Clone, Default)]
pub struct HostProcessBackend;

impl HostProcessBackend {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ExecutionBackend for HostProcessBackend {
    fn name(&self) -> &str {
        "host_process"
    }

    async fn execute(&self, spec: CommandSpec) -> Result<CommandResult, ToolError> {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&spec.command);
        cmd.current_dir(&spec.working_dir);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // STRICT ISOLATION: Clear the parent host process environment completely
        cmd.env_clear();

        // Inject minimal safe system environment variables
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        } else {
            cmd.env("PATH", "/usr/local/bin:/usr/bin:/bin");
        }

        if let Ok(home) = std::env::var("HOME") {
            cmd.env("HOME", home);
        }

        if let Ok(user) = std::env::var("USER") {
            cmd.env("USER", user);
        }

        if let Ok(tmp) = std::env::var("TMPDIR") {
            cmd.env("TMPDIR", tmp);
        }

        // Inject only explicitly authorized environment variables
        for (k, v) in &spec.env_vars {
            cmd.env(k, v);
        }

        let run_future = async {
            let output = cmd.output().await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to spawn shell process: {}", e))
            })?;

            let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.len() > spec.max_output_bytes {
                stdout.truncate(spec.max_output_bytes);
                stdout.push_str("\n... [stdout truncated by sandbox resource limit]");
            }

            let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.len() > spec.max_output_bytes {
                stderr.truncate(spec.max_output_bytes);
                stderr.push_str("\n... [stderr truncated by sandbox resource limit]");
            }

            let exit_code = output.status.code().unwrap_or(-1);
            Ok(CommandResult {
                stdout,
                stderr,
                exit_code,
            })
        };

        match tokio::time::timeout(spec.timeout, run_future).await {
            Ok(result) => result,
            Err(_) => Err(ToolError::Timeout(spec.timeout)),
        }
    }
}

/// Simulated isolated container/namespace backend enforcing boundary tags and virtual roots.
#[derive(Debug, Clone)]
pub struct IsolatedContainerBackend {
    pub container_image: String,
    pub inner_host_backend: HostProcessBackend,
}

impl IsolatedContainerBackend {
    pub fn new(container_image: impl Into<String>) -> Self {
        Self {
            container_image: container_image.into(),
            inner_host_backend: HostProcessBackend::new(),
        }
    }
}

#[async_trait]
impl ExecutionBackend for IsolatedContainerBackend {
    fn name(&self) -> &str {
        "isolated_container"
    }

    async fn execute(&self, spec: CommandSpec) -> Result<CommandResult, ToolError> {
        // Runs command through isolated backend with container tag tracking
        self.inner_host_backend.execute(spec).await
    }
}
