//! Project Workspace domain model for Plexis.
//!
//! A Workspace represents a developer project workspace bound to a canonical local filesystem path,
//! security policy, resource limits, and VCS tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::ids::WorkspaceId;

/// Confinement and security policy for a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSecurityPolicy {
    /// Allowed tool names or categories (e.g., ["filesystem", "git", "shell", "test"]).
    pub allowed_tools: Vec<String>,
    /// Whether execution outside the workspace canonical path is strictly blocked.
    pub enforce_confinement: bool,
    /// Sensitive files or path globs forbidden from access/modifications (e.g., [".env*", "*.pem", "id_rsa*"]).
    pub forbidden_patterns: Vec<String>,
    /// Require human approval before applying file modifications.
    pub require_approval_for_writes: bool,
    /// Require human approval before running arbitrary shell execution.
    pub require_approval_for_shell: bool,
}

impl Default for WorkspaceSecurityPolicy {
    fn default() -> Self {
        Self {
            allowed_tools: vec![
                "filesystem".to_string(),
                "git".to_string(),
                "shell".to_string(),
                "test".to_string(),
            ],
            enforce_confinement: true,
            forbidden_patterns: vec![
                ".env*".to_string(),
                "*.pem".to_string(),
                "*.key".to_string(),
                "id_rsa*".to_string(),
                "id_ed25519*".to_string(),
            ],
            require_approval_for_writes: false,
            require_approval_for_shell: true,
        }
    }
}

/// Resource constraints on workspace operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Max execution time per command/task in seconds.
    pub max_execution_time_secs: u64,
    /// Max memory allocation in megabytes (advisory/container limit).
    pub max_memory_mb: u64,
    /// Maximum patch/diff size permitted in bytes.
    pub max_diff_bytes: usize,
    /// Advisory cost budget in USD for LLM usage.
    pub max_cost_usd: f64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_execution_time_secs: 300,
            max_memory_mb: 2048,
            max_diff_bytes: 500_000,
            max_cost_usd: 10.0,
        }
    }
}

/// Git/VCS metadata attached to a workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VcsMetadata {
    /// Current git branch name.
    pub branch: Option<String>,
    /// Default remote URL (e.g., git@github.com:... or https://github.com/...).
    pub remote_url: Option<String>,
    /// Head commit hash.
    pub head_sha: Option<String>,
    /// Whether working tree has uncommitted changes.
    pub is_dirty: bool,
}

/// A first-class project workspace in Plexis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    /// Unique workspace identifier.
    pub id: WorkspaceId,
    /// Human-readable workspace name.
    pub name: String,
    /// Absolute canonical filesystem path to the root of the project.
    pub canonical_path: PathBuf,
    /// Confinement and security policy.
    pub policy: WorkspaceSecurityPolicy,
    /// Resource limits.
    pub limits: ResourceLimits,
    /// VCS status metadata.
    pub vcs: VcsMetadata,
    /// Flexible structured metadata.
    pub metadata: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Workspace {
    /// Creates a new workspace with default policy and limits.
    pub fn new(name: impl Into<String>, canonical_path: impl Into<PathBuf>) -> Self {
        let now = Utc::now();
        Self {
            id: WorkspaceId::new(),
            name: name.into(),
            canonical_path: canonical_path.into(),
            policy: WorkspaceSecurityPolicy::default(),
            limits: ResourceLimits::default(),
            vcs: VcsMetadata::default(),
            metadata: serde_json::Value::Object(Default::default()),
            created_at: now,
            updated_at: now,
        }
    }

    /// Builder to set custom security policy.
    pub fn with_policy(mut self, policy: WorkspaceSecurityPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Builder to set custom resource limits.
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Builder to set VCS metadata.
    pub fn with_vcs(mut self, vcs: VcsMetadata) -> Self {
        self.vcs = vcs;
        self
    }

    /// Builder to set arbitrary metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Validates whether a target path is allowed under this workspace's confinement and forbidden rules.
    pub fn is_path_allowed(&self, path: &Path) -> bool {
        // First resolve path relative to workspace root if relative
        let resolved = if path.is_relative() {
            self.canonical_path.join(path)
        } else {
            path.to_path_buf()
        };

        // Normalize / canonicalize if possible
        let normalized = if let Ok(canon) = resolved.canonicalize() {
            canon
        } else {
            // Lexical normalization for paths that might not yet exist
            let mut components = Vec::new();
            for comp in resolved.components() {
                match comp {
                    std::path::Component::ParentDir => {
                        components.pop();
                    }
                    std::path::Component::CurDir => {}
                    c => components.push(c),
                }
            }
            let mut p = PathBuf::new();
            for c in components {
                p.push(c.as_os_str());
            }
            p
        };

        // Check strict confinement
        if self.policy.enforce_confinement {
            let canon_root = self
                .canonical_path
                .canonicalize()
                .unwrap_or_else(|_| self.canonical_path.clone());
            if !normalized.starts_with(&canon_root) {
                return false;
            }
        }

        // Check forbidden patterns against path components
        if let Some(filename) = normalized.file_name().and_then(|s| s.to_str()) {
            for pattern in &self.policy.forbidden_patterns {
                if matches_pattern(pattern, filename) {
                    return false;
                }
            }
        }

        true
    }
}

/// Simple glob-style pattern matcher for forbidden filenames (e.g. "*.pem", ".env*")
fn matches_pattern(pattern: &str, text: &str) -> bool {
    if pattern == text {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        if text.starts_with(prefix) {
            return true;
        }
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        if text.ends_with(suffix) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_creation_and_defaults() {
        let ws = Workspace::new("my-project", "/tmp/my-project");
        assert_eq!(ws.name, "my-project");
        assert_eq!(ws.canonical_path, PathBuf::from("/tmp/my-project"));
        assert!(ws.policy.enforce_confinement);
        assert_eq!(ws.policy.allowed_tools.len(), 4);
    }

    #[test]
    fn test_confinement_checks() {
        let temp_dir = std::env::temp_dir();
        let ws_path = temp_dir.join("plexis_test_ws");
        let _ = std::fs::create_dir_all(&ws_path);
        let canonical_ws = ws_path.canonicalize().unwrap_or(ws_path.clone());
        let ws = Workspace::new("test", &canonical_ws);

        // Allowed inside
        let valid_file = canonical_ws.join("src").join("main.rs");
        assert!(ws.is_path_allowed(&valid_file));

        // Relative path inside
        assert!(ws.is_path_allowed(Path::new("Cargo.toml")));

        // Traversal outside
        let escape = canonical_ws.join("..").join("etc").join("passwd");
        assert!(!ws.is_path_allowed(&escape));

        // Forbidden file pattern
        let env_file = canonical_ws.join(".env.local");
        assert!(!ws.is_path_allowed(&env_file));

        let pem_file = canonical_ws.join("server.pem");
        assert!(!ws.is_path_allowed(&pem_file));
    }
}
