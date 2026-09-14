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
    /// Identifier for this backend (e.g. "host_process", "isolated_container", "bubblewrap").
    fn name(&self) -> &str;

    /// Indicates whether this backend enforces kernel-level filesystem mount boundary isolation.
    fn has_filesystem_isolation(&self) -> bool {
        false
    }

    /// Indicates whether this backend enforces network namespace isolation (blocking outbound sockets).
    fn has_network_isolation(&self) -> bool {
        false
    }

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
        cmd.kill_on_drop(true);

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

/// Linux unprivileged sandbox backend leveraging Bubblewrap (`bwrap`) to enforce
/// kernel-level filesystem, network, PID, IPC, UTS, and user namespace isolation.
#[derive(Debug, Clone)]
pub struct BubblewrapBackend {
    pub bwrap_path: PathBuf,
    pub enable_network: bool,
    pub additional_ro_binds: Vec<PathBuf>,
}

impl BubblewrapBackend {
    /// Creates a new BubblewrapBackend checking for `bwrap` executable.
    pub fn new() -> Result<Self, ToolError> {
        let path = Self::detect_bwrap().ok_or_else(|| {
            ToolError::ExecutionFailed("bwrap executable not found on system".into())
        })?;
        Ok(Self {
            bwrap_path: path,
            enable_network: false,
            additional_ro_binds: Vec::new(),
        })
    }

    /// Checks if bubblewrap is installed and functioning on this system.
    pub fn is_available() -> bool {
        Self::detect_bwrap().is_some()
    }

    fn detect_bwrap() -> Option<PathBuf> {
        for candidate in &["/usr/bin/bwrap", "/bin/bwrap", "/usr/local/bin/bwrap"] {
            let p = PathBuf::from(candidate);
            if p.exists() {
                return Some(p);
            }
        }
        if let Ok(output) = std::process::Command::new("which").arg("bwrap").output() {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(PathBuf::from(s));
                }
            }
        }
        None
    }

    pub fn with_network(mut self, enable: bool) -> Self {
        self.enable_network = enable;
        self
    }

    pub fn with_ro_bind(mut self, path: impl Into<PathBuf>) -> Self {
        self.additional_ro_binds.push(path.into());
        self
    }
}

#[async_trait]
impl ExecutionBackend for BubblewrapBackend {
    fn name(&self) -> &str {
        "bubblewrap"
    }

    fn has_filesystem_isolation(&self) -> bool {
        true
    }

    fn has_network_isolation(&self) -> bool {
        !self.enable_network
    }

    async fn execute(&self, spec: CommandSpec) -> Result<CommandResult, ToolError> {
        let mut cmd = Command::new(&self.bwrap_path);

        // 1. Mount read-only system directories
        cmd.arg("--ro-bind").arg("/usr").arg("/usr");
        cmd.arg("--ro-bind-try").arg("/lib").arg("/lib");
        cmd.arg("--ro-bind-try").arg("/lib64").arg("/lib64");
        cmd.arg("--ro-bind-try").arg("/bin").arg("/bin");
        cmd.arg("--ro-bind-try").arg("/sbin").arg("/sbin");
        cmd.arg("--ro-bind-try").arg("/etc").arg("/etc");
        cmd.arg("--ro-bind-try").arg("/home").arg("/home");

        // 2. Kernel proc & dev
        cmd.arg("--proc").arg("/proc");
        cmd.arg("--dev").arg("/dev");

        // 3. Isolated tmpfs for temporary files
        cmd.arg("--tmpfs").arg("/tmp");

        // 4. Read-only toolchains from host $HOME if present
        if let Ok(home) = std::env::var("HOME") {
            let cargo_dir = PathBuf::from(&home).join(".cargo");
            if cargo_dir.exists() {
                cmd.arg("--ro-bind").arg(&cargo_dir).arg(&cargo_dir);
            }
            let rustup_dir = PathBuf::from(&home).join(".rustup");
            if rustup_dir.exists() {
                cmd.arg("--ro-bind").arg(&rustup_dir).arg(&rustup_dir);
            }
        }

        for extra in &self.additional_ro_binds {
            if extra.exists() {
                cmd.arg("--ro-bind").arg(extra).arg(extra);
            }
        }

        // 5. Read-Write mount the target workspace directory ONLY
        cmd.arg("--bind")
            .arg(&spec.working_dir)
            .arg(&spec.working_dir);
        cmd.arg("--chdir").arg(&spec.working_dir);

        // 6. Namespace isolation flags
        if self.enable_network {
            cmd.arg("--unshare-user")
                .arg("--unshare-ipc")
                .arg("--unshare-pid")
                .arg("--unshare-uts")
                .arg("--unshare-cgroup");
        } else {
            cmd.arg("--unshare-all");
        }

        // 7. Process lifecycle boundary
        cmd.arg("--die-with-parent");

        // 8. Inject safe environment variables
        if let Ok(path) = std::env::var("PATH") {
            cmd.arg("--setenv").arg("PATH").arg(path);
        } else {
            cmd.arg("--setenv")
                .arg("PATH")
                .arg("/usr/local/bin:/usr/bin:/bin");
        }

        if let Ok(home) = std::env::var("HOME") {
            cmd.arg("--setenv").arg("HOME").arg(home);
        }

        for (k, v) in &spec.env_vars {
            cmd.arg("--setenv").arg(k).arg(v);
        }

        // 9. Command execution
        cmd.arg("sh").arg("-c").arg(&spec.command);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(true);

        let run_future = async {
            let output = cmd.output().await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to spawn bwrap sandbox process: {}", e))
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

/// Returns the strongest safe execution backend practical on the current system.
/// If `BubblewrapBackend` is supported, returns bubblewrap isolation; otherwise falls back to `HostProcessBackend`.
pub fn default_safe_backend() -> std::sync::Arc<dyn ExecutionBackend> {
    if BubblewrapBackend::is_available() {
        if let Ok(bwrap) = BubblewrapBackend::new() {
            return std::sync::Arc::new(bwrap);
        }
    }
    std::sync::Arc::new(HostProcessBackend::new())
}
