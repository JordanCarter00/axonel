use plexis_storage::SqliteStore;
use plexis_tools::ToolRegistry;
use std::sync::Arc;

/// Container for shared runtime and persistence resources.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqliteStore>,
    pub tool_registry: Arc<ToolRegistry>,
    pub auth_token: Option<String>,
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
        }
    }
}
