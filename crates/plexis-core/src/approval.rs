//! First-class human governance and approval models for Plexis.
//!
//! Provides durable records and state transitions for actions requiring
//! explicit human authorization or policy confirmation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, ApprovalId, TaskId, WorkflowId};

/// Current status of a human approval gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    /// Approval has been requested and is awaiting human decision.
    Pending,
    /// Approval was granted by a human operator.
    Approved,
    /// Approval was explicitly denied.
    Rejected,
}

impl ApprovalState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalState::Pending => "pending",
            ApprovalState::Approved => "approved",
            ApprovalState::Rejected => "rejected",
        }
    }
}

/// Durable record of an approval gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRecord {
    /// Unique approval identifier.
    pub id: ApprovalId,
    /// Associated task.
    pub task_id: TaskId,
    /// Associated workflow.
    pub workflow_id: WorkflowId,
    /// Agent that requested approval, if requested by an agent.
    pub requested_by: Option<AgentId>,
    /// Description of the action requiring approval.
    pub action_description: String,
    /// Current state.
    pub state: ApprovalState,
    /// Contextual notes, justification, or reason.
    pub reason: Option<String>,
    /// Timestamp when requested.
    pub created_at: DateTime<Utc>,
    /// Timestamp when decided by a human.
    pub decided_at: Option<DateTime<Utc>>,
}

impl ApprovalRecord {
    pub fn new(
        task_id: TaskId,
        workflow_id: WorkflowId,
        action_description: impl Into<String>,
    ) -> Self {
        Self {
            id: ApprovalId::new(),
            task_id,
            workflow_id,
            requested_by: None,
            action_description: action_description.into(),
            state: ApprovalState::Pending,
            reason: None,
            created_at: Utc::now(),
            decided_at: None,
        }
    }

    pub fn with_requester(mut self, agent_id: AgentId) -> Self {
        self.requested_by = Some(agent_id);
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn approve(&mut self, notes: Option<String>) {
        self.state = ApprovalState::Approved;
        self.decided_at = Some(Utc::now());
        if let Some(n) = notes {
            self.reason = Some(n);
        }
    }

    pub fn reject(&mut self, notes: Option<String>) {
        self.state = ApprovalState::Rejected;
        self.decided_at = Some(Utc::now());
        if let Some(n) = notes {
            self.reason = Some(n);
        }
    }
}
