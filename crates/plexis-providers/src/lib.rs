//! Plexis Providers
//!
//! Pluggable LLM provider abstractions and client adapters for OpenAI, Google Gemini,
//! Ollama, and test automation backends.
//!
//! This crate isolates all provider-specific wire schemas, authentication protocols,
//! error mappings, and token accounting away from the runtime and scheduler.

pub mod adapters;
pub mod capabilities;
pub mod error;
pub mod failover;
pub mod health;
pub mod reliability;
pub mod telemetry;
pub mod traits;
pub mod types;

// Re-exports
pub use adapters::{GeminiProvider, OllamaProvider, OpenAiProvider, ScriptedProvider};
pub use capabilities::{
    select_best_provider, standard_capability_matrix, ModelPricing, ProviderCapabilities,
    ReasoningTier,
};
pub use error::ProviderError;
pub use failover::{
    CapabilityRequirement, FailoverDecision, FailoverRouter, PrivacyPolicy, ProviderDescriptor,
};
pub use health::{ProviderHealthStatus, ProviderHealthTracker};
pub use reliability::RetryPolicy;
pub use telemetry::{
    ExecutionMode, LiveProviderProbe, ProviderExecutionTelemetry, ProviderTelemetryRecorder,
};
pub use traits::Provider;
pub use types::{
    ChatMessage, ChatRole, CompletionRequest, CompletionResponse, FinishReason, TokenUsage,
    ToolCall, ToolDefinition,
};
