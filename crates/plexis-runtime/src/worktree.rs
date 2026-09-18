//! Git Worktree Isolation and Workspace Management
//!
//! Provides dedicated isolated Git worktrees for concurrent external coding agents,
//! guarding against workspace collisions and enabling controlled branch integration.

use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

use crate::agent_host::security::WorkspaceValidator;
use crate::error::RuntimeError;

/// Details of a registered Git worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    /// Filesystem path of the worktree.
    pub path: PathBuf,
    /// Checked-out branch or commit.
    pub branch: Option<String>,
    /// Current HEAD commit SHA of the worktree.
    pub head_commit: Option<String>,
    /// Whether the worktree is locked.
    pub is_locked: bool,
}

/// Manages isolated Git worktrees for external agent executions.
#[derive(Debug, Clone)]
pub struct WorktreeManager {
    root_repo: PathBuf,
    worktrees_dir: PathBuf,
}

impl WorktreeManager {
    /// Creates a new WorktreeManager for a Git repository.
    pub fn new(root_repo: impl AsRef<Path>) -> Self {
        let root = root_repo.as_ref().to_path_buf();
        let default_dir = root.join(".plexis").join("worktrees");
        Self {
            root_repo: root,
            worktrees_dir: default_dir,
        }
    }

    /// Sets a custom base directory for worktrees.
    pub fn with_worktrees_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.worktrees_dir = dir.as_ref().to_path_buf();
        self
    }

    /// Root repository directory.
    pub fn root_repo(&self) -> &Path {
        &self.root_repo
    }

    /// Base directory where worktrees are mounted.
    pub fn worktrees_dir(&self) -> &Path {
        &self.worktrees_dir
    }

    /// Creates an isolated Git worktree on a dedicated branch.
    pub fn create_worktree(
        &self,
        branch_name: &str,
        base_ref: Option<&str>,
    ) -> Result<PathBuf, RuntimeError> {
        let sanitized = branch_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();

        let wt_path = self.worktrees_dir.join(&sanitized);

        // Ensure parent directory exists
        if let Some(parent) = wt_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                RuntimeError::Execution(format!(
                    "Failed to create worktree parent directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }

        // If worktree already exists, remove it first
        if wt_path.exists() {
            let _ = self.remove_worktree(&wt_path, false);
        }

        let base = base_ref.unwrap_or("HEAD");
        let output = Command::new("git")
            .current_dir(&self.root_repo)
            .args([
                "worktree",
                "add",
                "-B",
                branch_name,
                wt_path.to_str().unwrap_or(""),
                base,
            ])
            .output()
            .map_err(|e| {
                RuntimeError::Execution(format!("Failed to execute 'git worktree add': {}", e))
            })?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(RuntimeError::Execution(format!(
                "git worktree add failed for branch '{}' at {}: {}",
                branch_name,
                wt_path.display(),
                err_msg.trim()
            )));
        }

        // Validate and canonicalize worktree path
        let canonical = WorkspaceValidator::validate_and_canonicalize(&wt_path)?;
        info!(
            branch = branch_name,
            path = %canonical.display(),
            "Created isolated Git worktree for agent"
        );

        Ok(canonical)
    }

    /// Removes an existing worktree and optionally deletes the associated branch.
    pub fn remove_worktree(
        &self,
        worktree_path: &Path,
        delete_branch: bool,
    ) -> Result<(), RuntimeError> {
        let branch_name = if delete_branch {
            self.get_worktree_branch(worktree_path)
        } else {
            None
        };

        let output = Command::new("git")
            .current_dir(&self.root_repo)
            .args([
                "worktree",
                "remove",
                "--force",
                worktree_path.to_str().unwrap_or(""),
            ])
            .output()
            .map_err(|e| {
                RuntimeError::Execution(format!("Failed to execute 'git worktree remove': {}", e))
            })?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            warn!(
                path = %worktree_path.display(),
                err = %err_msg.trim(),
                "git worktree remove reported warning/error"
            );
            // Fallback manual cleanup if directory remains
            if worktree_path.exists() {
                let _ = std::fs::remove_dir_all(worktree_path);
            }
        }

        // Prune metadata
        let _ = Command::new("git")
            .current_dir(&self.root_repo)
            .args(["worktree", "prune"])
            .output();

        // Optionally delete associated branch
        if let Some(b) = branch_name {
            let _ = Command::new("git")
                .current_dir(&self.root_repo)
                .args(["branch", "-D", &b])
                .output();
        }

        Ok(())
    }

    /// Inspects the branch currently checked out in a worktree.
    pub fn get_worktree_branch(&self, worktree_path: &Path) -> Option<String> {
        let output = Command::new("git")
            .current_dir(worktree_path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()?;

        if output.status.success() {
            let b = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !b.is_empty() && b != "HEAD" {
                return Some(b);
            }
        }
        None
    }

    /// Inspects the current commit SHA of a worktree.
    pub fn get_worktree_commit(&self, worktree_path: &Path) -> Option<String> {
        let output = Command::new("git")
            .current_dir(worktree_path)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()?;

        if output.status.success() {
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sha.is_empty() {
                return Some(sha);
            }
        }
        None
    }

    /// Lists all active worktrees known to the repository.
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>, RuntimeError> {
        let output = Command::new("git")
            .current_dir(&self.root_repo)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .map_err(|e| {
                RuntimeError::Execution(format!("Failed to run 'git worktree list': {}", e))
            })?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(RuntimeError::Execution(format!(
                "git worktree list failed: {}",
                err
            )));
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();
        let mut cur_path: Option<PathBuf> = None;
        let mut cur_branch: Option<String> = None;
        let mut cur_commit: Option<String> = None;
        let mut cur_locked = false;

        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("worktree ") {
                if let Some(p) = cur_path.take() {
                    worktrees.push(WorktreeInfo {
                        path: p,
                        branch: cur_branch.take(),
                        head_commit: cur_commit.take(),
                        is_locked: cur_locked,
                    });
                    cur_locked = false;
                }
                cur_path = Some(PathBuf::from(rest.trim()));
            } else if let Some(rest) = line.strip_prefix("HEAD ") {
                cur_commit = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("branch refs/heads/") {
                cur_branch = Some(rest.trim().to_string());
            } else if line.starts_with("locked") {
                cur_locked = true;
            }
        }

        if let Some(p) = cur_path {
            worktrees.push(WorktreeInfo {
                path: p,
                branch: cur_branch,
                head_commit: cur_commit,
                is_locked: cur_locked,
            });
        }

        Ok(worktrees)
    }

    /// Integrates a verified agent branch into a target branch (e.g. master/main)
    /// in the root repository and returns the resulting commit SHA.
    pub fn integrate_branch(
        &self,
        source_branch: &str,
        target_branch: &str,
        commit_message: &str,
    ) -> Result<String, RuntimeError> {
        // 1. Checkout target branch in root repo
        let checkout_out = Command::new("git")
            .current_dir(&self.root_repo)
            .args(["checkout", target_branch])
            .output()
            .map_err(|e| {
                RuntimeError::Execution(format!("Failed to checkout target branch: {}", e))
            })?;

        if !checkout_out.status.success() {
            let err = String::from_utf8_lossy(&checkout_out.stderr);
            return Err(RuntimeError::Execution(format!(
                "Failed to checkout target branch '{}': {}",
                target_branch, err
            )));
        }

        // 2. Merge source branch with explicit message
        let merge_out = Command::new("git")
            .current_dir(&self.root_repo)
            .args(["merge", "--no-ff", "-m", commit_message, source_branch])
            .output()
            .map_err(|e| RuntimeError::Execution(format!("Failed to merge branch: {}", e)))?;

        if !merge_out.status.success() {
            let err = String::from_utf8_lossy(&merge_out.stderr);
            return Err(RuntimeError::Execution(format!(
                "Failed to merge branch '{}' into '{}': {}",
                source_branch, target_branch, err
            )));
        }

        // 3. Resolve final commit SHA
        let sha_out = Command::new("git")
            .current_dir(&self.root_repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| RuntimeError::Execution(format!("Failed to rev-parse HEAD: {}", e)))?;

        let sha = String::from_utf8_lossy(&sha_out.stdout).trim().to_string();
        info!(
            source = source_branch,
            target = target_branch,
            sha = %sha,
            "Integrated agent branch successfully"
        );

        Ok(sha)
    }

    /// Cleans up all managed worktrees and prunes Git references.
    pub fn cleanup_all(&self) -> Result<(), RuntimeError> {
        let worktrees = self.list_worktrees().unwrap_or_default();
        let canonical_root = self
            .root_repo
            .canonicalize()
            .unwrap_or_else(|_| self.root_repo.clone());

        for wt in worktrees {
            let canonical_wt = wt.path.canonicalize().unwrap_or_else(|_| wt.path.clone());
            // Do not delete the main repository worktree
            if canonical_wt != canonical_root {
                let _ = self.remove_worktree(&wt.path, true);
            }
        }

        let _ = Command::new("git")
            .current_dir(&self.root_repo)
            .args(["worktree", "prune"])
            .output();

        if self.worktrees_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.worktrees_dir);
        }

        Ok(())
    }
}
