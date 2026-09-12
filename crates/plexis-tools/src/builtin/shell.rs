//! Shell execution tool with capability authorization and process timeout boundaries.

use async_trait::async_trait;
use serde::Deserialize;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

use crate::error::ToolError;
use crate::sandbox::{AuthorizationResult, Capability};
use crate::traits::{Tool, ToolInvocationContext, ToolOutput};

/// Tool executing shell commands within an authorized working directory and time budget.
pub struct ShellTool;

#[derive(Deserialize)]
struct ShellArguments {
    command: String,
    timeout_seconds: Option<u64>,
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command within the sandboxed project workspace with execution timeout"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command line to execute in the workspace directory"
                },
                "timeout_seconds": {
                    "type": "integer",
                    "description": "Optional execution timeout in seconds"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, context: &ToolInvocationContext) -> Result<ToolOutput, ToolError> {
        let args: ShellArguments =
            serde_json::from_value(context.arguments.clone()).map_err(|e| {
                ToolError::InvalidArguments(format!("Failed to parse shell args: {}", e))
            })?;

        // 1. Authorize capability with sandbox
        let capability = Capability::ShellExec(args.command.clone());
        match context.sandbox.policy.authorize(&capability) {
            AuthorizationResult::Allowed => {}
            AuthorizationResult::Denied { reason } => {
                return Err(ToolError::PermissionDenied(reason));
            }
        }

        // 2. Determine timeout
        let timeout_duration = args
            .timeout_seconds
            .map(Duration::from_secs)
            .unwrap_or(context.sandbox.policy.limits.max_duration);

        // 3. Spawn child process
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(&args.command)
            .current_dir(&context.working_directory)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let run_future = async {
            let output = cmd.output().await.map_err(|e| {
                ToolError::ExecutionFailed(format!("Failed to spawn shell process: {}", e))
            })?;

            let max_bytes = context.sandbox.policy.limits.max_output_bytes;

            let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.len() > max_bytes {
                stdout.truncate(max_bytes);
                stdout.push_str("\n... [stdout truncated by sandbox resource limit]");
            }

            let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if stderr.len() > max_bytes {
                stderr.truncate(max_bytes);
                stderr.push_str("\n... [stderr truncated by sandbox resource limit]");
            }

            let exit_code = output.status.code().unwrap_or(-1);
            Ok(ToolOutput::process(stdout, stderr, exit_code))
        };

        match tokio::time::timeout(timeout_duration, run_future).await {
            Ok(result) => result,
            Err(_) => Err(ToolError::Timeout(timeout_duration)),
        }
    }
}
