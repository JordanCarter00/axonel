//! Persistent Agent Session domain model for Plexis.
//!
//! A Session represents the persistent execution continuity of an agent across
//! multiple turns and tasks. It survives process restarts, provider reconnects,
//! and application redeployments.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, SessionId};

/// Durable persistent agent session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Logical agent owning this session.
    pub agent_id: AgentId,
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
        self.closed_at.is_none()
    }

    pub fn close(&mut self) {
        let now = Utc::now();
        self.closed_at = Some(now);
        self.updated_at = now;
    }
}
