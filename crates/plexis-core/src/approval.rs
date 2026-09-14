//! First-class human governance and approval models for Plexis.
//!
//! Provides durable records and state transitions for actions requiring
//! explicit human authorization or policy confirmation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, ApprovalId, TaskId, WorkflowId};
use crate::state::StateTransitionError;

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

    pub fn can_transition_to(&self, next: &ApprovalState) -> bool {
        if self == next {
            return true;
        }
        match self {
            ApprovalState::Pending => {
                matches!(next, ApprovalState::Approved | ApprovalState::Rejected)
            }
            ApprovalState::Approved | ApprovalState::Rejected => false,
        }
    }

    pub fn transition_to(&mut self, next: ApprovalState) -> Result<(), StateTransitionError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(StateTransitionError {
                from: self.as_str().to_string(),
                to: next.as_str().to_string(),
                reason: "transition disallowed by approval lifecycle rules",
            })
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, ApprovalState::Approved | ApprovalState::Rejected)
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

    pub fn approve(&mut self, notes: Option<String>) -> Result<(), StateTransitionError> {
        self.state.transition_to(ApprovalState::Approved)?;
        self.decided_at = Some(Utc::now());
        if let Some(n) = notes {
            self.reason = Some(n);
        }
        Ok(())
    }

    pub fn reject(&mut self, notes: Option<String>) -> Result<(), StateTransitionError> {
        self.state.transition_to(ApprovalState::Rejected)?;
        self.decided_at = Some(Utc::now());
        if let Some(n) = notes {
            self.reason = Some(n);
        }
        Ok(())
    }
}
