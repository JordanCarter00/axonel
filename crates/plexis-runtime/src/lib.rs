//! Plexis Runtime
//!
//! Execution plane boundaries, command dispatching, lease management,
//! and reconciliation for the Plexis autonomous agent system.

pub mod dispatcher;
pub mod error;
pub mod lease_manager;
pub mod reconciler;
pub mod runner;
pub mod scheduler;
pub mod verifier;

pub use dispatcher::{BroadcastCommandDispatcher, CommandDispatcher};
pub use error::RuntimeError;
pub use lease_manager::LeaseManager;
pub use reconciler::{Reconciler, ReconciliationReport};
pub use runner::AgentRunner;
pub use scheduler::DeterministicScheduler;
pub use verifier::{VerificationContext, Verifier, WorkspaceVerifier};
