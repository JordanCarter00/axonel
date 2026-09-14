//! Security and sandbox isolation boundaries for tool execution.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::ToolError;

/// Individual capability requested during a tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    FileRead(PathBuf),
    FileWrite(PathBuf),
    ShellExec(String),
    GitOp(String),
}

/// Explicit permission flags granted to an agent or execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    ReadFileSystem,
    WriteFileSystem,
    ExecuteShell,
    ExecuteGit,
}

/// Resource constraints enforced by the sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_duration: Duration,
    pub max_output_bytes: usize,
    pub max_file_size_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_secs(30),
            max_output_bytes: 5 * 1024 * 1024,     // 5 MB
            max_file_size_bytes: 20 * 1024 * 1024, // 20 MB
        }
    }
}

/// Result of policy authorization check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorizationResult {
    Allowed,
    Denied { reason: String },
}

impl AuthorizationResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Durable security policy evaluated before every tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPolicy {
    pub allowed_read_roots: Vec<PathBuf>,
    pub allowed_write_roots: Vec<PathBuf>,
    pub allowed_commands: Option<Vec<String>>,
    pub allow_git: bool,
    pub limits: ResourceLimits,
}

impl SandboxPolicy {
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        let root = working_dir.into();
        Self {
            allowed_read_roots: vec![root.clone()],
            allowed_write_roots: vec![root],
            allowed_commands: None, // None means unrestricted within sandboxed dir
            allow_git: true,
            limits: ResourceLimits::default(),
        }
    }

    pub fn with_read_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.allowed_read_roots.push(root.into());
        self
    }

    pub fn with_write_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.allowed_write_roots.push(root.into());
        self
    }

    pub fn with_allowed_commands(mut self, commands: Vec<String>) -> Self {
        self.allowed_commands = Some(commands);
        self
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Evaluates whether a requested capability is permitted under this policy.
    pub fn authorize(&self, capability: &Capability) -> AuthorizationResult {
        match capability {
            Capability::FileRead(path) => {
                if is_contained_in_any(path, &self.allowed_read_roots) {
                    AuthorizationResult::Allowed
                } else {
                    AuthorizationResult::Denied {
                        reason: format!(
                            "Read access denied: path '{}' escapes allowed read roots",
                            path.display()
                        ),
                    }
                }
            }
            Capability::FileWrite(path) => {
                if is_contained_in_any(path, &self.allowed_write_roots) {
                    AuthorizationResult::Allowed
                } else {
                    AuthorizationResult::Denied {
                        reason: format!(
                            "Write access denied: path '{}' escapes allowed write roots",
                            path.display()
                        ),
                    }
                }
            }
            Capability::ShellExec(cmd) => {
                if let Some(whitelist) = &self.allowed_commands {
                    let cmd_name = cmd.split_whitespace().next().unwrap_or("");
                    if whitelist.iter().any(|allowed| allowed == cmd_name) {
                        AuthorizationResult::Allowed
                    } else {
                        AuthorizationResult::Denied {
                            reason: format!(
                                "Command '{}' is not in the allowed command whitelist",
                                cmd_name
                            ),
                        }
                    }
                } else {
                    AuthorizationResult::Allowed
                }
            }
            Capability::GitOp(_) => {
                if self.allow_git {
                    AuthorizationResult::Allowed
                } else {
                    AuthorizationResult::Denied {
                        reason: "Git operations are disallowed by sandbox policy".to_string(),
                    }
                }
            }
        }
    }
}

/// Helper verifying path containment to prevent directory traversal escapes.
fn is_contained_in_any(target: &Path, roots: &[PathBuf]) -> bool {
    let normalized = normalize_path(target);
    for root in roots {
        let normalized_root = normalize_path(root);
        if normalized.starts_with(&normalized_root) {
            return true;
        }
    }
    false
}

/// Path normalizer without requiring existing filesystem entries (safe against traversal).
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(p) => components.push(p.as_os_str().to_os_string()),
            std::path::Component::RootDir => components.push(std::ffi::OsString::from("/")),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if components.len() > 1 || (!components.is_empty() && components[0] != "/") {
                    components.pop();
                }
            }
            std::path::Component::Normal(c) => components.push(c.to_os_string()),
        }
    }
    if components.is_empty() {
        PathBuf::from(".")
    } else {
        let mut result = PathBuf::new();
        for (i, c) in components.into_iter().enumerate() {
            if i == 0 && c == "/" {
                result.push("/");
            } else {
                result.push(c);
            }
        }
        result
    }
}

/// Sandbox runtime instance attached to an agent execution session.
#[derive(Debug, Clone)]
pub struct Sandbox {
    pub root_directory: PathBuf,
    pub policy: SandboxPolicy,
}

impl Sandbox {
    pub fn new(root_directory: impl Into<PathBuf>) -> Self {
        let root = root_directory.into();
        let policy = SandboxPolicy::new(root.clone());
        Self {
            root_directory: root,
            policy,
        }
    }

    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Resolves a requested path and ensures it stays strictly within the sandbox boundaries.
    pub fn resolve_safe_path(
        &self,
        requested: &str,
        for_write: bool,
    ) -> Result<PathBuf, ToolError> {
        let path = Path::new(requested);
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root_directory.join(path)
        };

        let capability = if for_write {
            Capability::FileWrite(absolute.clone())
        } else {
            Capability::FileRead(absolute.clone())
        };

        match self.policy.authorize(&capability) {
            AuthorizationResult::Allowed => {
                let normalized = normalize_path(&absolute);

                // Canonicalize existing path or nearest parent to prevent symlink traversal escapes
                if let Ok(canonical_root) = self.root_directory.canonicalize() {
                    if normalized.exists() {
                        if let Ok(canonical_target) = normalized.canonicalize() {
                            if !canonical_target.starts_with(&canonical_root) {
                                return Err(ToolError::PermissionDenied(format!(
                                    "Symlink traversal escape detected: '{}' resolves to '{}' outside sandbox root '{}'",
                                    requested,
                                    canonical_target.display(),
                                    canonical_root.display()
                                )));
                            }
                        }
                    } else {
                        let mut curr = normalized.parent();
                        while let Some(parent) = curr {
                            if parent.exists() {
                                if let Ok(canonical_parent) = parent.canonicalize() {
                                    if !canonical_parent.starts_with(&canonical_root) {
                                        return Err(ToolError::PermissionDenied(format!(
                                            "Symlink parent traversal escape detected: '{}' parent resolves to '{}' outside sandbox root '{}'",
                                            requested,
                                            canonical_parent.display(),
                                            canonical_root.display()
                                        )));
                                    }
                                }
                                break;
                            }
                            curr = parent.parent();
                        }
                    }
                }

                Ok(normalized)
            }
            AuthorizationResult::Denied { reason } => Err(ToolError::PermissionDenied(reason)),
        }
    }
}
