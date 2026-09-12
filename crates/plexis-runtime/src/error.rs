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
}
