//! Shell execution tool with capability authorization, execution backends,
//! and automatic secret redaction.

use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

use crate::backend::CommandSpec;
use crate::error::ToolError;
use crate::redaction::SecretRedactor;
use crate::sandbox::{AuthorizationResult, Capability};
use crate::traits::{Tool, ToolInvocationContext, ToolOutput};

/// Tool executing shell commands within an authorized working directory, execution backend,
/// and time budget with automated secret redaction.
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
        "Execute a shell command within the sandboxed project workspace with execution timeout and secret isolation"
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

        // 3. Prepare authorized environment variables and redaction list
        let mut spec = CommandSpec::new(&args.command, &context.working_directory)
            .with_timeout(timeout_duration)
            .with_max_output_bytes(context.sandbox.policy.limits.max_output_bytes);

        let mut known_secrets = Vec::new();
        if let Some(ref store) = context.secret_store {
            let authorized_env = store.get_authorized_secrets(
                Some(&context.agent_id),
                Some(&context.task_id),
                Some("shell"),
            );
            for (k, v) in authorized_env {
                spec = spec.with_env(k, v);
            }
            known_secrets = store.all_secret_values();
        }

        // 4. Execute command through execution backend
        let backend = context.backend();
        let result = backend.execute(spec).await?;

        // 5. Redact secrets from stdout and stderr
        let clean_stdout = SecretRedactor::redact_text(&result.stdout, &known_secrets);
        let clean_stderr = SecretRedactor::redact_text(&result.stderr, &known_secrets);

        let mut output = ToolOutput::process(clean_stdout, clean_stderr, result.exit_code);
        SecretRedactor::redact_value(&mut output.data, &known_secrets);

        Ok(output)
    }
}
