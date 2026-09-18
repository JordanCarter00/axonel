//! Google Gemini CLI External Agent Subsystem
//!
//! Provides executable discovery, capability probing, stream translation,
//! and process supervision integration for the official Gemini CLI.

pub mod probe;

pub use probe::{GeminiAuthStatus, GeminiCapabilities, GeminiCapabilityProbe};
