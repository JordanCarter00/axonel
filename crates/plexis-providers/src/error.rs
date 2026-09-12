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
