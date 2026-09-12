//! Durable Task domain model for Plexis.
//!
//! Tasks are durable work units owned by Plexis, not by executing agents.
//! Task state and execution state are strictly decoupled.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, TaskId, WorkflowId};
use crate::state::{StateTransitionError, TaskState};

/// A durable unit of work to be planned, executed, and independently verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier.
    pub id: TaskId,
    /// Workflow this task belongs to.
    pub workflow_id: WorkflowId,
    /// High-level objective statement.
    pub objective: String,
    /// Detailed description and context.
    pub description: Option<String>,
    /// Optional parent task if this task was created via decomposition.
    pub parent_id: Option<TaskId>,
    /// Current lifecycle state.
    pub state: TaskState,
    /// Relative priority (higher values execute first).
    pub priority: i32,
    /// Acceptance criteria required for independent verification.
    pub criteria: Vec<String>,
    /// Currently assigned agent, if any.
    pub assigned_agent_id: Option<AgentId>,
    /// Number of completed or attempted execution runs.
    pub attempts: u32,
    /// Maximum allowed attempts before quarantining or human escalation.
    pub max_attempts: u32,
    /// Flexible structured metadata.
    pub metadata: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Task {
    /// Creates a new task in `Backlog` state with default parameters.
    pub fn new(workflow_id: WorkflowId, objective: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: TaskId::new(),
            workflow_id,
            objective: objective.into(),
            description: None,
            parent_id: None,
            state: TaskState::Backlog,
            priority: 0,
            criteria: Vec::new(),
            assigned_agent_id: None,
            attempts: 0,
            max_attempts: 3,
            metadata: serde_json::Value::Object(Default::default()),
            created_at: now,
            updated_at: now,
        }
    }

    /// Sets the task description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Sets the parent task ID (for subtasks created via decomposition).
    pub fn with_parent(mut self, parent_id: TaskId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Sets the task priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Sets acceptance criteria.
    pub fn with_criteria(mut self, criteria: Vec<String>) -> Self {
        self.criteria = criteria;
        self
    }

    /// Sets max allowed retry attempts.
    pub fn with_max_attempts(mut self, max: u32) -> Self {
        self.max_attempts = max;
        self
    }

    /// Sets the required capabilities in task metadata.
    pub fn with_required_capabilities(mut self, caps: Vec<String>) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.metadata {
            map.insert(
                "required_capabilities".to_string(),
                serde_json::to_value(caps).unwrap_or_default(),
            );
        }
        self
    }

    /// Returns the required capabilities declared in task metadata.
    pub fn required_capabilities(&self) -> Vec<String> {
        self.metadata
            .get("required_capabilities")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    /// Transitions task to a new state if valid, updating `updated_at`.
    pub fn transition_to(&mut self, next: TaskState) -> Result<(), StateTransitionError> {
        self.state.transition_to(next)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Assigns the task to an agent, transitioning state to `Assigned`.
    pub fn assign_to(&mut self, agent_id: AgentId) -> Result<(), StateTransitionError> {
        if self.state == TaskState::Backlog {
            self.transition_to(TaskState::Ready)?;
        }
        self.transition_to(TaskState::Assigned)?;
        self.assigned_agent_id = Some(agent_id);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Unassigns the task (e.g. after lease expiration), returning it to `Ready`.
    pub fn unassign(&mut self) -> Result<(), StateTransitionError> {
        self.transition_to(TaskState::Ready)?;
        self.assigned_agent_id = None;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Records an execution attempt increment.
    pub fn record_attempt(&mut self) {
        self.attempts += 1;
        self.updated_at = Utc::now();
    }
}
