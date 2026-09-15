//! Git version control tool for inspecting and recording changes in repositories.

use async_trait::async_trait;
use serde::Deserialize;
use std::process::Stdio;
use tokio::process::Command;

use crate::error::ToolError;
use crate::sandbox::{AuthorizationResult, Capability};
use crate::traits::{Tool, ToolInvocationContext, ToolOutput};

/// Tool for performing Git operations within an authorized workspace.
pub struct GitTool;

#[derive(Deserialize)]
struct GitArguments {
    action: Option<String>,
    subcommand: Option<String>,
    message: Option<String>,
    args: Option<Vec<String>>,
    max_commits: Option<usize>,
}

#[async_trait]
impl Tool for GitTool {
    fn name(&self) -> &str {
        "git"
    }

    fn description(&self) -> &str {
        "Perform Git version control operations (status, diff, log, commit) within the workspace"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["status", "diff", "log", "commit"],
                    "description": "Git subcommand to execute"
                },
                "message": {
                    "type": "string",
                    "description": "Commit message (required for 'commit' action)"
                },
                "max_commits": {
                    "type": "integer",
                    "description": "Maximum number of commits to list (for 'log' action, default 10)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, context: &ToolInvocationContext) -> Result<ToolOutput, ToolError> {
        context.validate_confinement()?;
        let args: GitArguments = serde_json::from_value(context.arguments.clone())
            .map_err(|e| ToolError::InvalidArguments(format!("Failed to parse git args: {}", e)))?;

        let action = args
            .action
            .or(args.subcommand)
            .unwrap_or_else(|| "status".to_string());

        // Authorize Git capability
        let capability = Capability::GitOp(action.clone());
        match context.sandbox.policy.authorize(&capability) {
            AuthorizationResult::Allowed => {}
            AuthorizationResult::Denied { reason } => {
                return Err(ToolError::PermissionDenied(reason));
            }
        }

        match action.as_str() {
            "status" => {
                let output = Command::new("git")
                    .arg("status")
                    .arg("--short")
                    .current_dir(&context.working_directory)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to run git status: {}", e))
                    })?;

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                Ok(ToolOutput::process(stdout, stderr, exit_code))
            }
            "diff" => {
                let output = Command::new("git")
                    .arg("diff")
                    .current_dir(&context.working_directory)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to run git diff: {}", e))
                    })?;

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                Ok(ToolOutput::process(stdout, stderr, exit_code))
            }
            "log" => {
                let limit = args.max_commits.unwrap_or(10);
                let output = Command::new("git")
                    .arg("log")
                    .arg(format!("-n{}", limit))
                    .arg("--oneline")
                    .current_dir(&context.working_directory)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to run git log: {}", e))
                    })?;

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                Ok(ToolOutput::process(stdout, stderr, exit_code))
            }
            "commit" => {
                let msg = args
                    .message
                    .or_else(|| {
                        args.args
                            .as_ref()
                            .and_then(|a| a.iter().rev().find(|s| !s.starts_with('-')).cloned())
                    })
                    .ok_or_else(|| {
                        ToolError::InvalidArguments("'message' is required for 'commit'".into())
                    })?;

                // git add -A
                let add_output = Command::new("git")
                    .arg("add")
                    .arg("-A")
                    .current_dir(&context.working_directory)
                    .output()
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to run git add: {}", e))
                    })?;

                if !add_output.status.success() {
                    let err = String::from_utf8_lossy(&add_output.stderr).to_string();
                    return Err(ToolError::ExecutionFailed(format!(
                        "git add failed: {}",
                        err
                    )));
                }

                // git commit -m msg with explicit agent author identity
                let commit_output = Command::new("git")
                    .arg("-c")
                    .arg("user.name=Plexis Agent")
                    .arg("-c")
                    .arg("user.email=agent@plexis.local")
                    .arg("commit")
                    .arg("-m")
                    .arg(&msg)
                    .current_dir(&context.working_directory)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await
                    .map_err(|e| {
                        ToolError::ExecutionFailed(format!("Failed to run git commit: {}", e))
                    })?;

                let stdout = String::from_utf8_lossy(&commit_output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&commit_output.stderr).to_string();
                let exit_code = commit_output.status.code().unwrap_or(-1);
                Ok(ToolOutput::process(stdout, stderr, exit_code))
            }
            unknown => Err(ToolError::InvalidArguments(format!(
                "Unknown git action: '{}'",
                unknown
            ))),
        }
    }
}
