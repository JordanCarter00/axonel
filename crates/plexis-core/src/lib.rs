//! Plexis Core
//!
//! Core domain models, strongly typed identifiers, state machines,
//! events, and task graph primitives for the Plexis autonomous agent system.
//!
//! This crate contains pure domain logic and invariants. It has zero
//! dependencies on persistence, networking, or specific LLM providers.

pub mod agent;
pub mod approval;
pub mod artifact;
pub mod command;
pub mod error;
pub mod event;
pub mod execution;
pub mod graph;
pub mod ids;
pub mod lease;
pub mod memory;
pub mod message;
pub mod planning;
pub mod recovery;
pub mod session;
pub mod state;
pub mod task;
pub mod verification;
pub mod workflow;
pub mod workspace;

// Convenient re-exports of core domain types
pub use agent::{Agent, ExecutionProfile};
pub use approval::{ApprovalRecord, ApprovalState};
pub use artifact::Artifact;
pub use command::{Command, CommandTarget, CommandType};
pub use error::CoreError;
pub use event::Event;
pub use execution::Execution;
pub use graph::{GraphError, TaskGraph};
pub use ids::{
    AgentId, ApprovalId, ArtifactId, CommandId, EventId, ExecutionId, LeaseId, MemoryId, MessageId,
    PlanId, RecoveryId, SessionId, TaskId, VerificationId, WorkflowId, WorkspaceId,
};
pub use lease::{Lease, LeaseError};
pub use memory::{MemoryProvenance, MemoryRecord, MemoryScope, MemoryState};
pub use message::{AgentMessage, MessageType};
pub use planning::{PlanStatus, PlanningRecord};
pub use recovery::{RecoveryRecord, RecoveryResult};
pub use session::{Session, SessionState};
pub use state::{
    AgentState, CommandState, ExecutionState, StateTransitionError, TaskState, WorkflowState,
};
pub use task::Task;
pub use verification::{Verification, VerificationVerdict};
pub use workflow::Workflow;
pub use workspace::{ResourceLimits, VcsMetadata, Workspace, WorkspaceSecurityPolicy};
