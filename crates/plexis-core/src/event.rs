//! Immutable Audit Event domain model for Plexis.
//!
//! Events serve as permanent evidence of state changes and decisions,
//! supporting replay, auditing, and observability without requiring full event-sourcing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::EventId;

/// An immutable audit event recording an action, state transition, or decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Unique event identifier.
    pub id: EventId,
    /// Type of aggregate affected (e.g. "task", "agent", "workflow", "command", "lease").
    pub aggregate_type: String,
    /// Identity of the aggregate (e.g. formatted TaskId).
    pub aggregate_id: String,
    /// Semantic event name (e.g. "task.created", "task.assigned", "command.dispatched").
    pub event_type: String,
    /// Detailed structured event payload.
    pub payload: serde_json::Value,
    /// Identity of actor initiating change (e.g. agent id, scheduler, user).
    pub actor: Option<String>,
    /// Identifier of the command or message that caused this event.
    pub causation_id: Option<String>,
    /// Top-level correlation trace identifier.
    pub correlation_id: Option<String>,
    /// Event occurrence timestamp.
    pub timestamp: DateTime<Utc>,
}

impl Event {
    pub fn new(
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: EventId::new(),
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.into(),
            event_type: event_type.into(),
            payload,
            actor: None,
            causation_id: None,
            correlation_id: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    pub fn with_causation(mut self, causation_id: impl Into<String>) -> Self {
        self.causation_id = Some(causation_id.into());
        self
    }

    pub fn with_correlation(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }
}
