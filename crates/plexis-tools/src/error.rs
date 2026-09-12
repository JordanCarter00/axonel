//! Error types for tool execution and sandbox boundaries in Plexis.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Tool authorization denied by sandbox policy: {0}")]
    PermissionDenied(String),

    #[error("Target entity or resource not found: {0}")]
    NotFound(String),

    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Tool execution timed out after {0:?}")]
    Timeout(std::time::Duration),

    #[error("Tool resource limit exceeded: {0}")]
    ResourceExceeded(String),

    #[error("Invalid arguments supplied to tool: {0}")]
    InvalidArguments(String),
}
