//! First-class agent-to-agent communication for Plexis.
//!
//! Agent communication is durable and asynchronous. Messages are stored
//! authoritatively by Plexis, surviving agent crashes or offline status.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, MessageId, TaskId, WorkflowId};

/// Semantic category of agent communication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Clarification question directed to another agent.
    Question,
    /// Work or resource request.
    Request,
    /// Result or data payload.
    Result,
    /// Explicit task or responsibility handoff.
    Handoff,
    /// Issue or cautionary advisory.
    Warning,
    /// Request for artifact or output review.
    Review,
    /// Rejection of proposed plan or work output.
    Rejection,
    /// Proposal for plan alteration, decomposition, or strategy change.
    Proposal,
    /// Durable reference to an artifact.
    ArtifactReference,
}

/// Durable message exchanged between agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique message identifier.
    pub id: MessageId,
    /// Sending agent.
    pub from_agent: AgentId,
    /// Intended recipient agent.
    pub to_agent: AgentId,
    /// Workflow context.
    pub workflow_id: WorkflowId,
    /// Associated task context if any.
    pub task_id: Option<TaskId>,
    /// Semantic intent.
    pub message_type: MessageType,
    /// Human-readable text message content.
    pub content: String,
    /// Structured payload (e.g. proposed subtasks, diffs, parameters).
    pub payload: serde_json::Value,
    /// Message timestamp.
    pub created_at: DateTime<Utc>,
}

impl AgentMessage {
    pub fn new(
        from_agent: AgentId,
        to_agent: AgentId,
        workflow_id: WorkflowId,
        message_type: MessageType,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: MessageId::new(),
            from_agent,
            to_agent,
            workflow_id,
            task_id: None,
            message_type,
            content: content.into(),
            payload: serde_json::Value::Null,
            created_at: Utc::now(),
        }
    }

    pub fn with_task(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }
}
