//! Error types for LLM provider adapters in Plexis.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Network error communicating with provider: {0}")]
    Network(String),

    #[error("Authentication failed for provider: {0}")]
    Authentication(String),

    #[error("Rate limited by provider (retry after {retry_after_secs:?} seconds)")]
    RateLimited { retry_after_secs: Option<u64> },

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Invalid or malformed response from provider: {0}")]
    InvalidResponse(String),

    #[error("Provider is currently unavailable: {0}")]
    Unavailable(String),

    #[error("Provider execution error: {0}")]
    ExecutionError(String),
}

impl ProviderError {
    /// Determines whether this error is transient and potentially recoverable with retry.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Network(_) => true,
            Self::RateLimited { .. } => true,
            Self::Unavailable(_) => true,
            Self::Authentication(_) => false,
            Self::ModelNotFound(_) => false,
            Self::InvalidResponse(_) => false,
            Self::ExecutionError(_) => false,
        }
    }

    /// Returns the recommended retry delay if explicitly signaled by the provider (e.g. Retry-After header).
    pub fn retry_after(&self) -> Option<std::time::Duration> {
        match self {
            Self::RateLimited {
                retry_after_secs: Some(secs),
            } => Some(std::time::Duration::from_secs(*secs)),
            _ => None,
        }
    }
}
