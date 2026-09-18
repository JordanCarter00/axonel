//! Runtime error definitions for Plexis.

use plexis_core::error::CoreError;
use plexis_storage::error::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Core(#[from] CoreError),

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("command dispatch error: {0}")]
    Dispatch(String),

    #[error("invalid command: {0}")]
    InvalidCommand(String),

    #[error("execution error: {0}")]
    Execution(String),

    #[error("lease error: {0}")]
    Lease(String),

    #[error("reconciliation error: {0}")]
    Reconciliation(String),

    #[error("verification error: {0}")]
    Verification(String),

    #[error("security error: {0}")]
    Security(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("mission error: {0}")]
    Mission(String),

    #[error("timeout error: {0}")]
    Timeout(String),
}

impl From<plexis_core::state::StateTransitionError> for RuntimeError {
    fn from(e: plexis_core::state::StateTransitionError) -> Self {
        RuntimeError::Core(e.into())
    }
}

