//! Shared application state for Plexis Server.

use plexis_storage::SqliteStore;

/// Container for shared runtime and persistence resources.
#[derive(Clone)]
pub struct AppState {
    pub store: SqliteStore,
}

impl AppState {
    pub fn new(store: SqliteStore) -> Self {
        Self { store }
    }
}
