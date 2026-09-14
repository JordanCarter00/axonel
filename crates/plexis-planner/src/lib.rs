//! Plexis Planner
//!
//! Autonomous planning, dynamic decomposition, structured proposal validation,
//! graph mutation application, and failure-aware diagnosis for Plexis.

pub mod applier;
pub mod diagnoser;
pub mod planner;
pub mod proposal;
pub mod validator;

pub use applier::{PlanApplicationResult, PlanApplier};
pub use diagnoser::{FailureCategory, FailureDiagnoser, RecoveryAction, RecoveryRecommendation};
pub use planner::{LlmPlanner, Planner, PlannerError, PlanningContext, ScriptedPlanner};
pub use proposal::{
    ExecutionStrategy, PlanProposal, ProposedDependency, ProposedTask, VerificationStrategy,
};
pub use validator::{PlanValidationReport, PlanValidator, PlannerBudgets};
