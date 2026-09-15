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
}
