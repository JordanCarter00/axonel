//! Failure recovery domain models, strategy identity, and audit records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{ExecutionId, RecoveryId, TaskId, WorkflowId};

/// Outcome result of an attempted failure recovery action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryResult {
    #[default]
    InProgress,
    Succeeded,
    Failed,
    Escalated,
}

impl RecoveryResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Escalated => "escalated",
        }
    }
}

impl std::fmt::Display for RecoveryResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for RecoveryResult {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "in_progress" => Ok(Self::InProgress),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "escalated" => Ok(Self::Escalated),
            other => Err(format!("Unknown recovery result: '{}'", other)),
        }
    }
}

/// Durable audit record tracking a failure, its diagnosis, and the recovery action applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub id: RecoveryId,
    pub task_id: TaskId,
    pub workflow_id: WorkflowId,
    pub execution_id: Option<ExecutionId>,
    pub attempt: u32,
    pub strategy: String,
    pub strategy_version: u32,
    pub failure_reason: String,
    pub diagnosis: serde_json::Value,
    pub recovery_action: String,
    pub action_reason: Option<String>,
    pub result: RecoveryResult,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RecoveryRecord {
    pub fn new(
        task_id: TaskId,
        workflow_id: WorkflowId,
        attempt: u32,
        strategy: impl Into<String>,
        failure_reason: impl Into<String>,
        recovery_action: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: RecoveryId::new(),
            task_id,
            workflow_id,
            execution_id: None,
            attempt,
            strategy: strategy.into(),
            strategy_version: 1,
            failure_reason: failure_reason.into(),
            diagnosis: serde_json::Value::Object(Default::default()),
            recovery_action: recovery_action.into(),
            action_reason: None,
            result: RecoveryResult::InProgress,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_execution_id(mut self, execution_id: ExecutionId) -> Self {
        self.execution_id = Some(execution_id);
        self
    }

    pub fn with_strategy_version(mut self, version: u32) -> Self {
        self.strategy_version = version;
        self
    }

    pub fn with_diagnosis(mut self, diagnosis: serde_json::Value) -> Self {
        self.diagnosis = diagnosis;
        self
    }

    pub fn with_action_reason(mut self, reason: impl Into<String>) -> Self {
        self.action_reason = Some(reason.into());
        self
    }

    pub fn mark_succeeded(&mut self) {
        self.result = RecoveryResult::Succeeded;
        self.updated_at = Utc::now();
    }

    pub fn mark_failed(&mut self) {
        self.result = RecoveryResult::Failed;
        self.updated_at = Utc::now();
    }

    pub fn mark_escalated(&mut self) {
        self.result = RecoveryResult::Escalated;
        self.updated_at = Utc::now();
    }
}
