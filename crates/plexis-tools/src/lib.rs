//! Plexis Tools
//!
//! Tool abstractions, security sandbox policy enforcement, capability authorization,
//! and invocation audit telemetry for the Plexis autonomous agent platform.

pub mod builtin;
pub mod error;
pub mod registry;
pub mod sandbox;
pub mod traits;

// Convenient re-exports
pub use builtin::{FilesystemTool, GitTool, ShellTool};
pub use error::ToolError;
pub use registry::ToolRegistry;
pub use sandbox::{
    AuthorizationResult, Capability, Permission, ResourceLimits, Sandbox, SandboxPolicy,
};
pub use traits::{Tool, ToolInvocationContext, ToolInvocationRecord, ToolOutput};
