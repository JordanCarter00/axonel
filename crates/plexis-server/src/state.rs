use plexis_providers::capabilities::{standard_capability_matrix, ProviderCapabilities};
use plexis_storage::SqliteStore;
use plexis_tools::ToolRegistry;
use std::sync::Arc;

use crate::github::{DefaultGitHubClient, GitHubIntegration};
use crate::terminal::TerminalBuffer;

/// Container for shared runtime and persistence resources.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqliteStore>,
    pub tool_registry: Arc<ToolRegistry>,
    pub auth_token: Option<String>,
    pub terminal_buffer: Arc<TerminalBuffer>,
    pub github: Arc<dyn GitHubIntegration>,
    pub capability_matrix: Vec<ProviderCapabilities>,
}

impl AppState {
    pub fn new(store: SqliteStore) -> Self {
        let auth_token = std::env::var("PLEXIS_AUTH_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty());
        Self {
            store: Arc::new(store),
            tool_registry: Arc::new(ToolRegistry::standard_suite()),
            auth_token,
            terminal_buffer: Arc::new(TerminalBuffer::new()),
            github: Arc::new(DefaultGitHubClient::new()),
            capability_matrix: standard_capability_matrix(),
        }
    }

    pub fn with_store(store: Arc<SqliteStore>) -> Self {
        let auth_token = std::env::var("PLEXIS_AUTH_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty());
        Self {
            store,
            tool_registry: Arc::new(ToolRegistry::standard_suite()),
            auth_token,
            terminal_buffer: Arc::new(TerminalBuffer::new()),
            github: Arc::new(DefaultGitHubClient::new()),
            capability_matrix: standard_capability_matrix(),
        }
    }

    pub fn with_auth_token(mut self, token: Option<String>) -> Self {
        self.auth_token = token;
        self
    }

    /// Evaluates candidate token using timing-attack-safe comparison,
    /// supporting token rotation via comma-separated active tokens.
    pub fn is_authorized_token(&self, candidate: &str) -> bool {
        let Some(ref token_spec) = self.auth_token else {
            return true;
        };
        for valid_token in token_spec
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            if constant_time_eq(candidate, valid_token) {
                return true;
            }
        }
        false
    }
}

/// Constant-time string equality check preventing timing attacks.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
