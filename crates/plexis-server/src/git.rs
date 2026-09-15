//! Git workspace operations and diff inspection service.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFileStatus {
    pub path: String,
    pub status: String, // "modified", "added", "deleted", "untracked", "renamed"
    pub staged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusResponse {
    pub branch: String,
    pub is_clean: bool,
    pub files: Vec<GitFileStatus>,
    pub head_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitDiffResponse {
    pub diff: String,
    pub files_changed: Vec<String>,
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitInfo {
    pub sha: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommitResult {
    pub sha: String,
    pub message: String,
}

/// Retrieves git status for a workspace path.
pub fn get_git_status(repo_path: &Path) -> Result<GitStatusResponse, String> {
    if !repo_path.exists() {
        return Err(format!("Path '{}' does not exist", repo_path.display()));
    }

    // 1. Get branch
    let branch_output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to run git rev-parse: {}", e))?;

    let branch = if branch_output.status.success() {
        String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string()
    } else {
        "unknown".to_string()
    };

    // 2. Get HEAD commit SHA
    let head_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok();

    let head_commit = head_output
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    // 3. Get porcelain status
    let status_output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to run git status: {}", e))?;

    let mut files = Vec::new();
    let status_str = String::from_utf8_lossy(&status_output.stdout);

    for line in status_str.lines() {
        if line.len() < 4 {
            continue;
        }
        let index_status = &line[0..1];
        let worktree_status = &line[1..2];
        let file_path = line[3..].trim().to_string();

        // Check index (staged)
        if index_status != " " && index_status != "?" {
            files.push(GitFileStatus {
                path: file_path.clone(),
                status: match index_status {
                    "M" => "modified",
                    "A" => "added",
                    "D" => "deleted",
                    "R" => "renamed",
                    _ => "changed",
                }
                .to_string(),
                staged: true,
            });
        }

        // Check worktree (unstaged / untracked)
        if worktree_status != " " {
            files.push(GitFileStatus {
                path: file_path,
                status: match worktree_status {
                    "M" => "modified",
                    "D" => "deleted",
                    "?" => "untracked",
                    _ => "changed",
                }
                .to_string(),
                staged: false,
            });
        }
    }

    let is_clean = files.is_empty();

    Ok(GitStatusResponse {
        branch,
        is_clean,
        files,
        head_commit,
    })
}

/// Retrieves git diff for a workspace path.
pub fn get_git_diff(repo_path: &Path, staged_only: bool) -> Result<GitDiffResponse, String> {
    if !repo_path.exists() {
        return Err(format!("Path '{}' does not exist", repo_path.display()));
    }

    let mut cmd = Command::new("git");
    cmd.current_dir(repo_path);

    if staged_only {
        cmd.args(["diff", "--cached"]);
    } else {
        // Compare working tree and index against HEAD so all modifications appear
        cmd.args(["diff", "HEAD"]);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    let diff_text = String::from_utf8_lossy(&output.stdout).to_string();

    let mut files_changed = Vec::new();
    let mut insertions = 0;
    let mut deletions = 0;

    for line in diff_text.lines() {
        if let Some(rest) = line.strip_prefix("diff --git a/") {
            if let Some(file) = rest.split_whitespace().next() {
                files_changed.push(file.to_string());
            }
        } else if line.starts_with('+') && !line.starts_with("+++") {
            insertions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }

    Ok(GitDiffResponse {
        diff: diff_text,
        files_changed,
        insertions,
        deletions,
    })
}

/// Retrieves commit history log for a workspace path.
pub fn get_git_log(repo_path: &Path, limit: usize) -> Result<Vec<GitCommitInfo>, String> {
    if !repo_path.exists() {
        return Err(format!("Path '{}' does not exist", repo_path.display()));
    }

    let limit_str = format!("-n{}", limit.max(1));
    let output = Command::new("git")
        .args([
            "log",
            &limit_str,
            "--pretty=format:%H%x00%an%x00%ae%x00%at%x00%s",
        ])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to run git log: {}", e))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();

    for line in raw.lines() {
        let parts: Vec<&str> = line.split('\0').collect();
        if parts.len() >= 5 {
            let timestamp: i64 = parts[3].parse().unwrap_or(0);
            commits.push(GitCommitInfo {
                sha: parts[0].to_string(),
                author_name: parts[1].to_string(),
                author_email: parts[2].to_string(),
                timestamp,
                message: parts[4].to_string(),
            });
        }
    }

    Ok(commits)
}

/// Commits all changes in the workspace path.
pub fn commit_git_changes(repo_path: &Path, message: &str) -> Result<GitCommitResult, String> {
    if message.trim().is_empty() {
        return Err("Commit message cannot be empty".to_string());
    }

    // git add -A
    let add_status = Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_path)
        .status()
        .map_err(|e| format!("Failed to run git add: {}", e))?;

    if !add_status.success() {
        return Err("git add -A failed".to_string());
    }

    // git commit -m <message>
    let commit_output = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to run git commit: {}", e))?;

    if !commit_output.status.success() {
        let stderr = String::from_utf8_lossy(&commit_output.stderr);
        let stdout = String::from_utf8_lossy(&commit_output.stdout);
        return Err(format!("git commit failed: {} {}", stdout, stderr));
    }

    // Retrieve new commit SHA
    let sha_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to get commit SHA: {}", e))?;

    let sha = String::from_utf8_lossy(&sha_output.stdout)
        .trim()
        .to_string();

    Ok(GitCommitResult {
        sha,
        message: message.to_string(),
    })
}

/// Discovers VCS metadata for a repository path.
pub fn discover_git_metadata(repo_path: &Path) -> Result<plexis_core::VcsMetadata, String> {
    let st = get_git_status(repo_path)?;
    let remote_output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(repo_path)
        .output();
    let remote_url = match remote_output {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        _ => None,
    };
    Ok(plexis_core::VcsMetadata {
        branch: if st.branch.is_empty() {
            None
        } else {
            Some(st.branch)
        },
        remote_url,
        head_sha: st.head_commit,
        is_dirty: !st.is_clean,
    })
}
