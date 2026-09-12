//! Plexis Providers
//!
//! Pluggable LLM provider abstractions and client adapters for OpenAI, Google Gemini,
//! Ollama, and test automation backends.
//!
//! This crate isolates all provider-specific wire schemas, authentication protocols,
//! error mappings, and token accounting away from the runtime and scheduler.

pub mod adapters;
pub mod error;
pub mod traits;
pub mod types;

// Re-exports
pub use adapters::{GeminiProvider, OllamaProvider, OpenAiProvider, ScriptedProvider};
pub use error::ProviderError;
pub use traits::Provider;
pub use types::{
    ChatMessage, ChatRole, CompletionRequest, CompletionResponse, FinishReason, TokenUsage,
    ToolCall, ToolDefinition,
};
