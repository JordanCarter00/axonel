//! Provider abstraction trait for Plexis.

use async_trait::async_trait;

use crate::error::ProviderError;
use crate::types::{CompletionRequest, CompletionResponse};

/// Primary trait implemented by all LLM provider adapters.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Identifier of the provider (e.g. "openai", "gemini", "ollama", "scripted").
    fn id(&self) -> &str;

    /// Dispatches a completion request and returns the normalized completion response.
    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;
}
