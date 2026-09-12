//! Plexis Tools
//!
//! Tool abstractions, security sandbox policy enforcement, capability authorization,
//! and invocation audit telemetry for the Plexis autonomous agent platform.

pub mod backend;
pub mod builtin;
pub mod error;
pub mod redaction;
pub mod registry;
pub mod sandbox;
pub mod secrets;
pub mod traits;

// Convenient re-exports
pub use backend::{
    CommandResult, CommandSpec, ExecutionBackend, HostProcessBackend, IsolatedContainerBackend,
};
pub use builtin::{FilesystemTool, GitTool, ShellTool};
pub use error::ToolError;
pub use redaction::SecretRedactor;
pub use registry::ToolRegistry;
pub use sandbox::{
    AuthorizationResult, Capability, Permission, ResourceLimits, Sandbox, SandboxPolicy,
};
pub use secrets::{InMemorySecretStore, SecretAcl, SecretStore};
pub use traits::{Tool, ToolInvocationContext, ToolInvocationRecord, ToolOutput};
