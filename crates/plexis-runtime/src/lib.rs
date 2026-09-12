//! Plexis Runtime
//!
//! Execution plane boundaries, command dispatching, lease management,
//! and reconciliation for the Plexis autonomous agent system.

pub mod dispatcher;
pub mod error;
pub mod lease_manager;
pub mod reconciler;

pub use dispatcher::{BroadcastCommandDispatcher, CommandDispatcher};
pub use error::RuntimeError;
pub use lease_manager::LeaseManager;
pub use reconciler::{Reconciler, ReconciliationReport};
