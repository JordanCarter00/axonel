use plexis_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Embedding dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("Invalid embedding data: {0}")]
    InvalidEmbedding(String),

    #[error("Memory record not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}
