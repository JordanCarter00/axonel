//! Plexis Storage
//!
//! Persistence abstractions, transactional boundaries, and SQLite repository
//! implementations for the Plexis autonomous agent system.

pub mod error;
pub mod sqlite;
pub mod traits;

pub use error::StorageError;
pub use sqlite::SqliteStore;
pub use traits::{
    AgentStore, CommandStore, EventStore, ExecutionStore, LeaseStore, SessionStore, TaskStore,
    VerificationStore, WorkflowStore,
};
