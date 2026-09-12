//! Logical Agent domain model for Plexis.
//!
//! An Agent is a durable logical execution participant. Agent identity is distinct
//! from provider, model, process, terminal, or session.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, ExecutionId};
use crate::state::AgentState;

/// Execution profile decoupling logical agent identity from AI providers and models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionProfile {
    /// Target provider adapter (e.g. "openai", "gemini", "ollama").
    pub provider: String,
    /// Requested model name (e.g. "gpt-4o", "gemini-1.5-pro").
    pub model: String,
    /// Provider-specific hyper-parameters (temperature, max_tokens, etc.).
    pub parameters: serde_json::Value,
}

impl ExecutionProfile {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            parameters: serde_json::Value::Object(Default::default()),
        }
    }
}

/// Durable logical agent in Plexis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    /// Unique agent identifier.
    pub id: AgentId,
    /// Human-readable display name.
    pub display_name: String,
    /// Logical role (e.g. "Architect", "Code Implementer", "Verifier").
    pub role: String,
    /// Execution profile directing provider and model resolution.
    pub provider_profile: ExecutionProfile,
    /// Declared capabilities (e.g. "rust", "frontend", "code-review").
    pub capabilities: Vec<String>,
    /// Security and tool permissions granted to this agent.
    pub permissions: Vec<String>,
    /// Current activity status.
    pub state: AgentState,
    /// Active execution run if currently busy.
    pub current_execution_id: Option<ExecutionId>,
    /// Flexible structured configuration.
    pub configuration: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Agent {
    /// Creates a new Agent with an idle state.
    pub fn new(
        display_name: impl Into<String>,
        role: impl Into<String>,
        provider_profile: ExecutionProfile,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: AgentId::new(),
            display_name: display_name.into(),
            role: role.into(),
            provider_profile,
            capabilities: Vec::new(),
            permissions: Vec::new(),
            state: AgentState::Idle,
            current_execution_id: None,
            configuration: serde_json::Value::Object(Default::default()),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_permissions(mut self, perms: Vec<String>) -> Self {
        self.permissions = perms;
        self
    }

    pub fn set_state(&mut self, state: AgentState) {
        self.state = state;
        self.updated_at = Utc::now();
    }

    pub fn bind_execution(&mut self, exec_id: ExecutionId) {
        self.current_execution_id = Some(exec_id);
        self.state = AgentState::Busy;
        self.updated_at = Utc::now();
    }

    pub fn clear_execution(&mut self) {
        self.current_execution_id = None;
        self.state = AgentState::Idle;
        self.updated_at = Utc::now();
    }
}
