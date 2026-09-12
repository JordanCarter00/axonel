//! Durable Command subsystem for Plexis.
//!
//! Commands form the durable asynchronous bridge between the control plane
//! (planner/scheduler) and the execution plane (agent runtime/tools).
//! Everything important is idempotent.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, CommandId, TaskId, WorkflowId};
use crate::state::CommandState;

/// Target entity for a durable command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target_type", content = "target_id", rename_all = "snake_case")]
pub enum CommandTarget {
    Agent(AgentId),
    Task(TaskId),
    Workflow(WorkflowId),
    System,
}

/// Semantic command action to be executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandType {
    /// Instructs an agent runtime to begin executing a task.
    ExecuteTask,
    /// Delivers an agent-to-agent message.
    SendMessage,
    /// Triggers independent verification for a task.
    VerifyTask,
    /// Requests peer or human review of an artifact.
    ReviewArtifact,
    /// Instructs an agent to continue its execution turn.
    ContinueAgent,
    /// Administratively pauses an agent execution.
    PauseAgent,
    /// Administratively resumes a paused agent execution.
    ResumeAgent,
    /// Cancels an in-flight execution.
    CancelExecution,
    /// Creates and spins up a new logical agent.
    CreateAgent,
}

/// A durable, idempotent command record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Unique command identifier.
    pub id: CommandId,
    /// Target receiving this command.
    pub target: CommandTarget,
    /// Action requested.
    pub command_type: CommandType,
    /// Structured payload specific to command type.
    pub payload: serde_json::Value,
    /// Current lifecycle state.
    pub state: CommandState,
    /// Explicit idempotency key preventing duplicate dispatch.
    pub idempotency_key: String,
    /// Number of dispatch/delivery attempts so far.
    pub attempts: u32,
    /// Maximum retry attempts before moving to DeadLettered.
    pub max_attempts: u32,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Timestamp when command was dispatched.
    pub dispatched_at: Option<DateTime<Utc>>,
    /// Timestamp when command completed or failed permanently.
    pub completed_at: Option<DateTime<Utc>>,
}

impl Command {
    /// Creates a new queued command with an idempotency key.
    pub fn new(
        target: CommandTarget,
        command_type: CommandType,
        payload: serde_json::Value,
        idempotency_key: impl Into<String>,
    ) -> Self {
        Self {
            id: CommandId::new(),
            target,
            command_type,
            payload,
            state: CommandState::Queued,
            idempotency_key: idempotency_key.into(),
            attempts: 0,
            max_attempts: 3,
            created_at: Utc::now(),
            dispatched_at: None,
            completed_at: None,
        }
    }

    /// Marks the command as dispatched.
    pub fn mark_dispatched(&mut self) {
        self.state = CommandState::Dispatched;
        self.attempts += 1;
        self.dispatched_at = Some(Utc::now());
    }

    /// Marks the command as confirmed / succeeded.
    pub fn mark_confirmed(&mut self) {
        self.state = CommandState::Confirmed;
        self.completed_at = Some(Utc::now());
    }

    /// Alias for mark_confirmed.
    pub fn mark_completed(&mut self) {
        self.mark_confirmed();
    }

    /// Marks the command as failed, incrementing or dead-lettering depending on max_attempts.
    pub fn mark_failed(&mut self) {
        if self.attempts >= self.max_attempts {
            self.state = CommandState::DeadLettered;
            self.completed_at = Some(Utc::now());
        } else {
            self.state = CommandState::Retrying;
        }
    }
}
