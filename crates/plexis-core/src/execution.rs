//! Durable Execution domain model for Plexis.
//!
//! An Execution represents a concrete attempt to run a task by an agent,
//! decoupling ephemeral process runs from durable task identity.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, ExecutionId, LeaseId, SessionId, TaskId};
use crate::state::{ExecutionState, StateTransitionError};

/// Concrete attempt by an agent to execute a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Execution {
    /// Unique execution run identifier.
    pub id: ExecutionId,
    /// Target task being executed.
    pub task_id: TaskId,
    /// Assigned agent executing this run.
    pub agent_id: AgentId,
    /// Associated persistent session if part of a long-running session.
    pub session_id: Option<SessionId>,
    /// Active lease under which this execution was initiated.
    pub lease_id: Option<LeaseId>,
    /// Current execution state.
    pub state: ExecutionState,
    /// Attempt number (1-indexed).
    pub attempt: u32,
    /// Timestamp when live process work actually began.
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp when execution finished (succeeded, failed, or timed out).
    pub completed_at: Option<DateTime<Utc>>,
    /// Failure error message if execution did not succeed.
    pub error_message: Option<String>,
    /// Structured metadata (metrics, token counts, process pid).
    pub metadata: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Execution {
    /// Creates a new pending execution attempt.
    pub fn new(task_id: TaskId, agent_id: AgentId, attempt: u32) -> Self {
        let now = Utc::now();
        Self {
            id: ExecutionId::new(),
            task_id,
            agent_id,
            session_id: None,
            lease_id: None,
            state: ExecutionState::Pending,
            attempt,
            started_at: None,
            completed_at: None,
            error_message: None,
            metadata: serde_json::Value::Object(Default::default()),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_session(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_lease(mut self, lease_id: LeaseId) -> Self {
        self.lease_id = Some(lease_id);
        self
    }

    /// Marks execution as running.
    pub fn mark_running(&mut self) -> Result<(), StateTransitionError> {
        self.state.transition_to(ExecutionState::Running)?;
        let now = Utc::now();
        self.started_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    /// Marks execution as completed.
    pub fn mark_completed(&mut self) -> Result<(), StateTransitionError> {
        self.state.transition_to(ExecutionState::Completed)?;
        let now = Utc::now();
        self.completed_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    /// Marks execution as failed with error details.
    pub fn mark_failed(&mut self, reason: impl Into<String>) -> Result<(), StateTransitionError> {
        self.state.transition_to(ExecutionState::Failed)?;
        let now = Utc::now();
        self.completed_at = Some(now);
        self.error_message = Some(reason.into());
        self.updated_at = now;
        Ok(())
    }

    /// Marks execution as timed out.
    pub fn mark_timed_out(&mut self) -> Result<(), StateTransitionError> {
        self.state.transition_to(ExecutionState::TimedOut)?;
        let now = Utc::now();
        self.completed_at = Some(now);
        self.error_message = Some("execution duration budget exceeded".to_string());
        self.updated_at = now;
        Ok(())
    }

    /// Marks execution as cancelled.
    pub fn mark_cancelled(&mut self) -> Result<(), StateTransitionError> {
        self.state.transition_to(ExecutionState::Cancelled)?;
        let now = Utc::now();
        self.completed_at = Some(now);
        self.updated_at = now;
        Ok(())
    }
}
