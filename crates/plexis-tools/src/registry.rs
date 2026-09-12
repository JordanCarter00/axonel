//! Central tool registry managing discovery, schemas, and authorized execution.

use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;

use crate::builtin::{FilesystemTool, GitTool, ShellTool};
use crate::error::ToolError;
use crate::sandbox::AuthorizationResult;
use crate::traits::{Tool, ToolInvocationContext, ToolInvocationRecord, ToolOutput};

/// Central catalog of callable tools.
#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Initializes registry with the standard default tool suite (filesystem, shell, git).
    pub fn standard_suite() -> Self {
        let mut registry = Self::new();
        registry.register(Arc::new(FilesystemTool));
        registry.register(Arc::new(ShellTool));
        registry.register(Arc::new(GitTool));
        registry
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.values().cloned().collect()
    }

    /// Invokes a named tool through sandbox authorization and produces a durable invocation audit record.
    pub async fn invoke(
        &self,
        tool_name: &str,
        context: &ToolInvocationContext,
    ) -> (ToolInvocationRecord, Result<ToolOutput, ToolError>) {
        let started_at = Utc::now();

        let tool = match self.get(tool_name) {
            Some(t) => t,
            None => {
                let err_msg = format!("Tool '{}' not registered in registry", tool_name);
                let record = ToolInvocationRecord {
                    agent_id: context.agent_id,
                    execution_id: context.execution_id,
                    task_id: context.task_id,
                    tool_name: tool_name.to_string(),
                    arguments: context.arguments.clone(),
                    authorization_result: AuthorizationResult::Denied {
                        reason: err_msg.clone(),
                    },
                    started_at,
                    ended_at: Some(Utc::now()),
                    result: None,
                    failure: Some(err_msg.clone()),
                };
                return (record, Err(ToolError::NotFound(err_msg)));
            }
        };

        let result = tool.execute(context).await;
        let ended_at = Utc::now();

        let (auth_result, failure, output) = match &result {
            Ok(out) => (AuthorizationResult::Allowed, None, Some(out.clone())),
            Err(ToolError::PermissionDenied(reason)) => (
                AuthorizationResult::Denied {
                    reason: reason.clone(),
                },
                Some(reason.clone()),
                None,
            ),
            Err(e) => (AuthorizationResult::Allowed, Some(e.to_string()), None),
        };

        let record = ToolInvocationRecord {
            agent_id: context.agent_id,
            execution_id: context.execution_id,
            task_id: context.task_id,
            tool_name: tool_name.to_string(),
            arguments: context.arguments.clone(),
            authorization_result: auth_result,
            started_at,
            ended_at: Some(ended_at),
            result: output,
            failure,
        };

        (record, result)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::standard_suite()
    }
}
