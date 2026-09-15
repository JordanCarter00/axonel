//! External Agent Host Subsystem
//!
//! Supervises external autonomous coding agent OS processes across a physical
//! process boundary with workspace containment, environment scrubbing, process-group
//! isolation, streaming output, timeout, and cancellation.

pub mod host;
pub mod security;

pub use host::{ActiveProcess, LocalAgentHost, ProcessState};
pub use security::{EnvironmentScrubber, WorkspaceValidator};
