//! Plexis Runtime
//!
//! Execution plane boundaries, command dispatching, lease management,
//! and reconciliation for the Plexis autonomous agent system.

pub mod agent_host;
pub mod backend;
pub mod context;
pub mod dispatcher;
pub mod error;
pub mod governance;
pub mod lease_manager;
pub mod observability;
pub mod reconciler;
pub mod recovery;
pub mod recovery_harness;
pub mod resources;
pub mod runner;
pub mod scheduler;
pub mod selector;
pub mod verifier;
pub mod workload;
pub mod worktree;

pub use agent_host::{
    ActiveProcess, EnvironmentScrubber, LocalAgentHost, ProcessState, WorkspaceValidator,
};
pub use backend::{
    AgentBackend, BackendRegistry, ClaudeCodeBackend, CodexBackend, FakeAgentBackend,
    GeminiCliBackend, OpenCodeBackend,
};
pub use context::{
    ContextBudget, ContextBuilder, ContextProvenance, ContextProvenanceItem, ContextSummary,
    OmissionReceipt,
};
pub use dispatcher::{BroadcastCommandDispatcher, CommandDispatcher};
pub use error::RuntimeError;
pub use governance::GovernanceManager;
pub use lease_manager::LeaseManager;
pub use observability::{
    AuditEvent, AuditSeverity, ProvenanceAuditTrail, RunStats, RunSummary, RunSummaryBuilder,
    SecurityBoundaryAudit,
};
pub use reconciler::{Reconciler, ReconciliationReport};
pub use recovery::{RecoveryAction, RecoveryController};
pub use recovery_harness::CrashResumptionHarness;
pub use resources::{
    ConcurrencyLimiter, ConcurrencyPermit, ResourceLimitsConfig, TaskResourceTracker,
};
pub use runner::AgentRunner;
pub use scheduler::DeterministicScheduler;
pub use selector::AgentSelector;
pub use verifier::{VerificationContext, Verifier, WorkspaceVerifier};
pub use workload::CanonicalWorkload;
pub use worktree::{WorktreeInfo, WorktreeManager};
