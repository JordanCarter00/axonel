//! Workflow domain model for Plexis.
//!
//! A Workflow represents a high-level goal comprising multiple tasks,
//! dependencies, executions, and verification steps.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{WorkflowId, WorkspaceId};
use crate::state::WorkflowState;

/// High-level workflow orchestrating interrelated tasks to accomplish an objective.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique workflow identifier.
    pub id: WorkflowId,
    /// Associated workspace, if any.
    pub workspace_id: Option<WorkspaceId>,
    /// Human-readable title.
    pub title: String,
    /// Broad objective statement for the workflow.
    pub objective: String,
    /// Current workflow lifecycle state.
    pub state: WorkflowState,
    /// Arbitrary structured metadata.
    pub metadata: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Workflow {
    pub fn new(title: impl Into<String>, objective: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: WorkflowId::new(),
            workspace_id: None,
            title: title.into(),
            objective: objective.into(),
            state: WorkflowState::Draft,
            metadata: serde_json::Value::Object(Default::default()),
            created_at: now,
            updated_at: now,
        }
    }

    /// Sets the associated workspace.
    pub fn with_workspace_id(mut self, workspace_id: WorkspaceId) -> Self {
        self.workspace_id = Some(workspace_id);
        self
    }

    pub fn set_state(&mut self, state: WorkflowState) {
        self.state = state;
        self.updated_at = Utc::now();
    }
}
