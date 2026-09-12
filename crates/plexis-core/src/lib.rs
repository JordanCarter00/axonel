//! Plexis Core
//!
//! Core domain models, strongly typed identifiers, state machines,
//! events, and task graph primitives for the Plexis autonomous agent system.
//!
//! This crate contains pure domain logic and invariants. It has zero
//! dependencies on persistence, networking, or specific LLM providers.

pub mod agent;
pub mod artifact;
pub mod command;
pub mod error;
pub mod event;
pub mod graph;
pub mod ids;
pub mod lease;
pub mod message;
pub mod state;
pub mod task;
pub mod verification;
pub mod workflow;

// Convenient re-exports of core domain types
pub use agent::{Agent, ExecutionProfile};
pub use artifact::Artifact;
pub use command::{Command, CommandTarget, CommandType};
pub use error::CoreError;
pub use event::Event;
pub use graph::{GraphError, TaskGraph};
pub use ids::{
    AgentId, ArtifactId, CommandId, EventId, ExecutionId, LeaseId, MessageId, SessionId,
    TaskId, VerificationId, WorkflowId,
};
pub use lease::{Lease, LeaseError};
pub use message::{AgentMessage, MessageType};
pub use state::{
    AgentState, CommandState, ExecutionState, StateTransitionError, TaskState, WorkflowState,
};
pub use task::Task;
pub use verification::{Verification, VerificationVerdict};
pub use workflow::Workflow;
