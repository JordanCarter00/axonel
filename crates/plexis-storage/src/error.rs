//! Storage errors for Plexis.

use plexis_core::ids::IdParseError;
use plexis_core::state::StateTransitionError;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("serialization/deserialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("invalid identifier: {0}")]
    IdParse(#[from] IdParseError),

    #[error("state transition error: {0}")]
    StateTransition(#[from] StateTransitionError),

    #[error("entity not found: {entity_type} with id '{id}'")]
    NotFound {
        entity_type: &'static str,
        id: String,
    },

    #[error("idempotency conflict: command with key '{0}' already exists")]
    IdempotencyConflict(String),

    #[error("lease conflict: task '{0}' already has an active lease")]
    LeaseConflict(String),

    #[error("migration error: {0}")]
    Migration(String),
}
