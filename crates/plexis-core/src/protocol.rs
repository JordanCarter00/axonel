//! External Agent Process Protocol
//!
//! Strongly typed, versioned IPC protocol structures exchanged between
//! the Plexis Local Agent Host and external coding-agent processes.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, ExecutionId};

/// Current supported external agent IPC protocol version.
pub const PROTOCOL_VERSION_V1: &str = "1.0";

/// Structured execution request sent from the Local Agent Host to an external agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRequest {
    /// Protocol version for forwards/backwards compatibility.
    pub protocol_version: String,
    /// Authoritative execution ID assigned by Plexis.
    pub execution_id: ExecutionId,
    /// Logical agent ID executing the task.
    pub agent_id: AgentId,
    /// Declared role of the agent (e.g. "Developer", "Tester").
    pub role: String,
    /// High-level engineering objective statement.
    pub objective: String,
    /// Detailed description, acceptance criteria, and context.
    pub description: Option<String>,
    /// Validated canonical path to the target workspace.
    pub workspace_path: PathBuf,
    /// Scrubbed environment variables provided to the process.
    pub environment: HashMap<String, String>,
    /// Execution timeout in seconds.
    pub timeout_secs: u64,
    /// Declared capabilities requested for this execution run.
    pub requested_capabilities: Vec<String>,
    /// Optional deliberate failure mode for testing recovery and resilience.
    /// Supported values: "crash", "non_zero_exit", "hang", "syntax_error".
    pub failure_mode: Option<String>,
    /// Optional deliberate execution delay in milliseconds (for cancellation/timeout tests).
    pub delay_ms: Option<u64>,
    /// Execution policy governing tool execution permissions (e.g. "read_only", "workspace_edit", "full_autonomous").
    #[serde(default)]
    pub execution_policy: Option<String>,
    /// Selected LLM model if requested (e.g. "gemini-2.5-pro", "gemini-2.5-flash").
    #[serde(default)]
    pub model: Option<String>,
    /// Structured correlation metadata (workflow ID, task ID, trace ID).
    pub correlation_metadata: serde_json::Value,
}

impl ExecutionRequest {
    /// Creates a new execution request with protocol version 1.0.
    pub fn new(
        execution_id: ExecutionId,
        agent_id: AgentId,
        role: impl Into<String>,
        objective: impl Into<String>,
        workspace_path: PathBuf,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_V1.to_string(),
            execution_id,
            agent_id,
            role: role.into(),
            objective: objective.into(),
            description: None,
            workspace_path,
            environment: HashMap::new(),
            timeout_secs: 300,
            requested_capabilities: Vec::new(),
            failure_mode: None,
            delay_ms: None,
            execution_policy: None,
            model: None,
            correlation_metadata: serde_json::Value::Object(Default::default()),
        }
    }

    pub fn with_execution_policy(mut self, policy: impl Into<String>) -> Self {
        self.execution_policy = Some(policy.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    pub fn with_environment(mut self, env: HashMap<String, String>) -> Self {
        self.environment = env;
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.requested_capabilities = caps;
        self
    }

    pub fn with_failure_mode(mut self, mode: impl Into<String>) -> Self {
        self.failure_mode = Some(mode.into());
        self
    }

    pub fn with_delay_ms(mut self, delay_ms: u64) -> Self {
        self.delay_ms = Some(delay_ms);
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.correlation_metadata = metadata;
        self
    }
}

/// Granular event types emitted by an external agent during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ExecutionEventType {
    /// The external agent process has initialized and recorded its OS PID.
    Started { pid: u32 },
    /// Standard output line emitted by the agent or its child processes.
    Stdout { text: String },
    /// Standard error line emitted by the agent or its child processes.
    Stderr { text: String },
    /// A discrete tool invocation or action performed by the agent.
    ToolAction {
        tool: String,
        action: String,
        details: serde_json::Value,
    },
    /// Execution progress update.
    Progress { percentage: f32, message: String },
    /// Non-fatal diagnostic warning.
    Warning { message: String },
    /// Agent finished execution successfully.
    Completed,
    /// Agent failed during execution.
    Failed {
        error: String,
        exit_code: Option<i32>,
    },
    /// Agent acknowledged cancellation and aborted cleanly.
    Cancelled,
}

/// A timestamped, serializable event emitted across the process protocol boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionEvent {
    /// Protocol version.
    pub protocol_version: String,
    /// Execution ID this event belongs to.
    pub execution_id: ExecutionId,
    /// UTC timestamp of event generation.
    pub timestamp: DateTime<Utc>,
    /// Event payload.
    pub event: ExecutionEventType,
}

impl ExecutionEvent {
    pub fn new(execution_id: ExecutionId, event: ExecutionEventType) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_V1.to_string(),
            execution_id,
            timestamp: Utc::now(),
            event,
        }
    }

    pub fn started(execution_id: ExecutionId, pid: u32) -> Self {
        Self::new(execution_id, ExecutionEventType::Started { pid })
    }

    pub fn stdout(execution_id: ExecutionId, text: impl Into<String>) -> Self {
        Self::new(
            execution_id,
            ExecutionEventType::Stdout { text: text.into() },
        )
    }

    pub fn stderr(execution_id: ExecutionId, text: impl Into<String>) -> Self {
        Self::new(
            execution_id,
            ExecutionEventType::Stderr { text: text.into() },
        )
    }

    pub fn tool_action(
        execution_id: ExecutionId,
        tool: impl Into<String>,
        action: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self::new(
            execution_id,
            ExecutionEventType::ToolAction {
                tool: tool.into(),
                action: action.into(),
                details,
            },
        )
    }

    pub fn progress(
        execution_id: ExecutionId,
        percentage: f32,
        message: impl Into<String>,
    ) -> Self {
        Self::new(
            execution_id,
            ExecutionEventType::Progress {
                percentage,
                message: message.into(),
            },
        )
    }

    pub fn warning(execution_id: ExecutionId, message: impl Into<String>) -> Self {
        Self::new(
            execution_id,
            ExecutionEventType::Warning {
                message: message.into(),
            },
        )
    }

    pub fn completed(execution_id: ExecutionId) -> Self {
        Self::new(execution_id, ExecutionEventType::Completed)
    }

    pub fn failed(
        execution_id: ExecutionId,
        error: impl Into<String>,
        exit_code: Option<i32>,
    ) -> Self {
        Self::new(
            execution_id,
            ExecutionEventType::Failed {
                error: error.into(),
                exit_code,
            },
        )
    }

    pub fn cancelled(execution_id: ExecutionId) -> Self {
        Self::new(execution_id, ExecutionEventType::Cancelled)
    }
}

/// Final structured outcome returned upon external agent process termination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Protocol version.
    pub protocol_version: String,
    /// Execution ID this result corresponds to.
    pub execution_id: ExecutionId,
    /// Process exit code (0 for success, non-zero for failure).
    pub exit_code: i32,
    /// Whether the external agent objective was achieved.
    pub success: bool,
    /// Human-readable summary of work performed.
    pub summary: String,
    /// List of file paths modified or created relative to the workspace root.
    pub changed_files: Vec<String>,
    /// Git commit SHA if the agent created a commit.
    pub commit_sha: Option<String>,
    /// Total duration of the process execution in milliseconds.
    pub duration_ms: u64,
    /// Error message if execution was unsuccessful.
    pub failure_reason: Option<String>,
    /// Captured raw standard output.
    pub raw_stdout: Option<String>,
    /// Captured raw standard error.
    pub raw_stderr: Option<String>,
}

impl ExecutionResult {
    /// Creates a successful execution result.
    pub fn success(
        execution_id: ExecutionId,
        summary: impl Into<String>,
        changed_files: Vec<String>,
        commit_sha: Option<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_V1.to_string(),
            execution_id,
            exit_code: 0,
            success: true,
            summary: summary.into(),
            changed_files,
            commit_sha,
            duration_ms,
            failure_reason: None,
            raw_stdout: None,
            raw_stderr: None,
        }
    }

    /// Creates a failed execution result.
    pub fn failure(
        execution_id: ExecutionId,
        exit_code: i32,
        reason: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        let reason_str = reason.into();
        Self {
            protocol_version: PROTOCOL_VERSION_V1.to_string(),
            execution_id,
            exit_code,
            success: false,
            summary: format!("Execution failed: {}", reason_str),
            changed_files: Vec::new(),
            commit_sha: None,
            duration_ms,
            failure_reason: Some(reason_str),
            raw_stdout: None,
            raw_stderr: None,
        }
    }

    /// Sets raw captured stdout and stderr.
    pub fn with_raw_output(mut self, stdout: Option<String>, stderr: Option<String>) -> Self {
        self.raw_stdout = stdout;
        self.raw_stderr = stderr;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_request_serde_roundtrip() {
        let exec_id = ExecutionId::new();
        let agent_id = AgentId::new();
        let req = ExecutionRequest::new(
            exec_id,
            agent_id,
            "Developer",
            "Fix bug in modulo calculation",
            PathBuf::from("/tmp/repo"),
        )
        .with_description("Fix negative numbers")
        .with_timeout_secs(60)
        .with_failure_mode("none");

        let json = serde_json::to_string(&req).expect("serialize");
        let deserialized: ExecutionRequest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(req, deserialized);
        assert_eq!(deserialized.protocol_version, PROTOCOL_VERSION_V1);
    }

    #[test]
    fn test_execution_event_serde_roundtrip() {
        let exec_id = ExecutionId::new();
        let ev = ExecutionEvent::tool_action(
            exec_id,
            "filesystem",
            "write_file",
            serde_json::json!({ "path": "src/lib.rs" }),
        );

        let json = serde_json::to_string(&ev).expect("serialize");
        let deserialized: ExecutionEvent = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ev, deserialized);
    }

    #[test]
    fn test_execution_result_success_and_failure() {
        let exec_id = ExecutionId::new();
        let succ = ExecutionResult::success(
            exec_id,
            "Bug resolved and verified",
            vec!["src/lib.rs".into()],
            Some("abcdef0123456789".into()),
            1250,
        );
        assert!(succ.success);
        assert_eq!(succ.exit_code, 0);
        assert_eq!(succ.commit_sha.as_deref(), Some("abcdef0123456789"));

        let fail = ExecutionResult::failure(exec_id, 1, "cargo test failed", 850);
        assert!(!fail.success);
        assert_eq!(fail.exit_code, 1);
        assert_eq!(fail.failure_reason.as_deref(), Some("cargo test failed"));
    }
}
