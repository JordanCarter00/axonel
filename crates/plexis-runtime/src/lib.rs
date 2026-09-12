//! Plexis Runtime
//!
//! Execution plane boundaries, command dispatching, lease management,
//! and reconciliation for the Plexis autonomous agent system.

pub mod context;
pub mod dispatcher;
pub mod error;
pub mod governance;
pub mod lease_manager;
pub mod reconciler;
pub mod recovery;
pub mod resources;
pub mod runner;
pub mod scheduler;
pub mod selector;
pub mod verifier;

pub use context::{
    ContextBudget, ContextBuilder, ContextProvenance, ContextProvenanceItem, ContextSummary,
    OmissionReceipt,
};
pub use dispatcher::{BroadcastCommandDispatcher, CommandDispatcher};
pub use error::RuntimeError;
pub use governance::GovernanceManager;
pub use lease_manager::LeaseManager;
pub use reconciler::{Reconciler, ReconciliationReport};
pub use recovery::{RecoveryAction, RecoveryController};
pub use resources::{
    ConcurrencyLimiter, ConcurrencyPermit, ResourceLimitsConfig, TaskResourceTracker,
};
pub use runner::AgentRunner;
pub use scheduler::DeterministicScheduler;
pub use selector::AgentSelector;
pub use verifier::{VerificationContext, Verifier, WorkspaceVerifier};
