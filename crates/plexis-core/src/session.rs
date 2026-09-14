//! Persistent Agent Session domain model for Plexis.
//!
//! A Session represents the persistent execution continuity of an agent across
//! multiple turns and tasks. It survives process restarts, provider reconnects,
//! and application redeployments.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, SessionId};
use crate::state::StateTransitionError;

/// Lifecycle state of an agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Active,
    Paused,
    Closed,
}

impl SessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Closed => "closed",
        }
    }

    pub fn can_transition_to(&self, next: &SessionState) -> bool {
        if self == next {
            return true;
        }
        match self {
            Self::Active => matches!(next, Self::Paused | Self::Closed),
            Self::Paused => matches!(next, Self::Active | Self::Closed),
            Self::Closed => false,
        }
    }

    pub fn transition_to(&mut self, next: SessionState) -> Result<(), StateTransitionError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(StateTransitionError {
                from: self.as_str().to_string(),
                to: next.as_str().to_string(),
                reason: "transition disallowed by session lifecycle rules",
            })
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// Durable persistent agent session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Logical agent owning this session.
    pub agent_id: AgentId,
    /// Current session lifecycle state.
    pub state: SessionState,
    /// Optional external provider conversation/session identifier (e.g. OpenAI thread id).
    pub provider_session_id: Option<String>,
    /// Working directory for local sandboxed execution.
    pub working_directory: Option<String>,
    /// Flexible structured metadata.
    pub metadata: serde_json::Value,
    /// Session start timestamp.
    pub created_at: DateTime<Utc>,
    /// Last session interaction timestamp.
    pub updated_at: DateTime<Utc>,
    /// Timestamp when session was explicitly terminated or closed.
    pub closed_at: Option<DateTime<Utc>>,
}

impl Session {
    /// Creates a new active session for an agent.
    pub fn new(agent_id: AgentId) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::new(),
            agent_id,
            state: SessionState::Active,
            provider_session_id: None,
            working_directory: None,
            metadata: serde_json::Value::Object(Default::default()),
            created_at: now,
            updated_at: now,
            closed_at: None,
        }
    }

    pub fn with_working_directory(mut self, path: impl Into<String>) -> Self {
        self.working_directory = Some(path.into());
        self
    }

    pub fn with_provider_session_id(mut self, id: impl Into<String>) -> Self {
        self.provider_session_id = Some(id.into());
        self
    }

    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active && self.closed_at.is_none()
    }

    pub fn close(&mut self) -> Result<(), StateTransitionError> {
        self.state.transition_to(SessionState::Closed)?;
        let now = Utc::now();
        self.closed_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), StateTransitionError> {
        self.state.transition_to(SessionState::Paused)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), StateTransitionError> {
        self.state.transition_to(SessionState::Active)?;
        self.updated_at = Utc::now();
        Ok(())
    }
}
