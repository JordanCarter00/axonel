//! Tool abstraction and invocation telemetry models for Plexis.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use plexis_core::ids::{AgentId, ExecutionId, TaskId};

use crate::error::ToolError;
use crate::sandbox::{AuthorizationResult, Sandbox};

/// Execution output from a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Structured output data or json-formatted payload.
    pub data: serde_json::Value,
    /// Human-readable stdout stream if applicable.
    pub stdout: Option<String>,
    /// Human-readable stderr stream if applicable.
    pub stderr: Option<String>,
    /// Process exit code if applicable.
    pub exit_code: Option<i32>,
}

impl ToolOutput {
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            data,
            stdout: None,
            stderr: None,
            exit_code: Some(0),
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        let content = text.into();
        Self {
            data: serde_json::json!({ "result": content }),
            stdout: Some(content),
            stderr: None,
            exit_code: Some(0),
        }
    }

    pub fn process(stdout: String, stderr: String, exit_code: i32) -> Self {
        Self {
            data: serde_json::json!({
                "stdout": stdout,
                "stderr": stderr,
                "exit_code": exit_code,
            }),
            stdout: Some(stdout),
            stderr: Some(stderr),
            exit_code: Some(exit_code),
        }
    }
}

/// Execution context passed to an individual tool invocation.
pub struct ToolInvocationContext {
    pub agent_id: AgentId,
    pub execution_id: ExecutionId,
    pub task_id: TaskId,
    pub arguments: serde_json::Value,
    pub sandbox: Arc<Sandbox>,
    pub working_directory: PathBuf,
}

/// Durable audit record capturing everything about a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocationRecord {
    pub agent_id: AgentId,
    pub execution_id: ExecutionId,
    pub task_id: TaskId,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub authorization_result: AuthorizationResult,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub result: Option<ToolOutput>,
    pub failure: Option<String>,
}

/// Common trait implemented by all Plexis tools.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name of the tool (e.g. "filesystem", "shell", "git").
    fn name(&self) -> &str;

    /// Human/model description of the tool capability.
    fn description(&self) -> &str;

    /// JSON Schema definition of accepted parameters.
    fn schema(&self) -> serde_json::Value;

    /// Executes the tool within the provided sandboxed context.
    async fn execute(&self, context: &ToolInvocationContext) -> Result<ToolOutput, ToolError>;
}
