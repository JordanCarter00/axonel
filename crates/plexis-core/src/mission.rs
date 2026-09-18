//! Mission domain model for long-horizon autonomous engineering goals.
//!
//! A Mission is the supervisory coordinator above Workflow. It manages multi-cycle execution,
//! durable checkpoints, liveness/health tracking, resource budget enforcement, and independent
//! stopping condition verification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{CheckpointId, ExecutionId, MissionId, TaskId, WorkflowId, WorkspaceId};
use crate::state::MissionState;

/// Health and liveness evaluation for an active mission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MissionHealth {
    /// Mission is making active observable progress.
    #[default]
    Healthy,
    /// Mission has executed cycles without measurable progress in tasks or commits.
    Stagnant,
    /// An active agent or task has ceased heartbeat or execution.
    Stalled,
    /// Recovery attempts or replanning iterations are accumulating.
    Degraded,
    /// Escalated to human operator due to policy, security, or repeated failures.
    Escalated,
}

/// Configurable resource and operational budget limits for a mission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionBudget {
    /// Maximum duration of the mission in seconds (default 1 hour).
    pub max_duration_secs: u64,
    /// Maximum number of external agent processes running concurrently.
    pub max_concurrent_agents: usize,
    /// Maximum total external process executions across all cycles.
    pub max_executions: u32,
    /// Maximum failure recovery attempts before escalating or failing.
    pub max_recovery_attempts: u32,
    /// Maximum adaptive replanning iterations allowed.
    pub max_planner_iterations: u32,
    /// Maximum consecutive stagnant cycles without observable progress.
    pub max_stagnant_cycles: u32,
}

impl Default for MissionBudget {
    fn default() -> Self {
        Self {
            max_duration_secs: 3600,
            max_concurrent_agents: 4,
            max_executions: 20,
            max_recovery_attempts: 5,
            max_planner_iterations: 10,
            max_stagnant_cycles: 3,
        }
    }
}

/// Current resource consumption recorded against the mission budget.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionBudgetConsumed {
    /// Total elapsed execution duration in seconds.
    pub duration_secs: u64,
    /// Total external process executions launched so far.
    pub total_executions: u32,
    /// Total recovery attempts triggered.
    pub recovery_attempts: u32,
    /// Total planner iterations executed.
    pub planner_iterations: u32,
    /// Consecutive stagnant cycles observed without progress.
    pub stagnant_cycles: u32,
}

impl MissionBudgetConsumed {
    /// Checks whether any configured limit has been exhausted.
    /// Returns `Some(reason)` if a budget is breached, or `None` if healthy.
    pub fn has_exhausted(&self, budget: &MissionBudget) -> Option<String> {
        if self.duration_secs >= budget.max_duration_secs {
            return Some(format!(
                "Maximum mission duration of {}s exceeded (consumed {}s)",
                budget.max_duration_secs, self.duration_secs
            ));
        }
        if self.total_executions >= budget.max_executions {
            return Some(format!(
                "Maximum process executions limit of {} reached (launched {})",
                budget.max_executions, self.total_executions
            ));
        }
        if self.recovery_attempts >= budget.max_recovery_attempts {
            return Some(format!(
                "Maximum recovery attempts of {} reached (triggered {})",
                budget.max_recovery_attempts, self.recovery_attempts
            ));
        }
        if self.planner_iterations >= budget.max_planner_iterations {
            return Some(format!(
                "Maximum planner iterations of {} reached (run {})",
                budget.max_planner_iterations, self.planner_iterations
            ));
        }
        if self.stagnant_cycles >= budget.max_stagnant_cycles {
            return Some(format!(
                "Maximum consecutive stagnant cycles of {} exceeded ({} cycles without progress)",
                budget.max_stagnant_cycles, self.stagnant_cycles
            ));
        }
        None
    }
}

/// Explicit verifiable completion predicates for a mission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoppingCondition {
    /// Required automated cargo test suite passes completely.
    pub required_tests_pass: bool,
    /// Git working tree must be clean with zero untracked/dirty files.
    pub working_tree_clean: bool,
    /// An authoritative new Git commit must be present.
    pub required_commit_exists: bool,
    /// Optional custom verification command (e.g. `cargo clippy -- -D warnings`).
    pub custom_verifier: Option<String>,
}

impl Default for StoppingCondition {
    fn default() -> Self {
        Self {
            required_tests_pass: true,
            working_tree_clean: true,
            required_commit_exists: true,
            custom_verifier: None,
        }
    }
}

/// Final outcome details recorded when a mission completes or terminates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionOutcome {
    /// Whether the engineering objective was successfully achieved.
    pub success: bool,
    /// Comprehensive summary of achievements, discoveries, and verified changes.
    pub summary: String,
    /// The verified Git commit SHA representing the mission's authoritative deliverable.
    pub verified_commit_sha: Option<String>,
    /// Total autonomous cycles executed.
    pub cycles_count: u32,
    /// Authoritative reason for mission termination.
    pub completion_reason: String,
}

/// Long-horizon Mission coordinating multi-cycle workflows, recovery, and budgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mission {
    /// Unique mission identifier.
    pub id: MissionId,
    /// Associated workspace root, if any.
    pub workspace_id: Option<WorkspaceId>,
    /// Currently active workflow driving execution tasks.
    pub active_workflow_id: Option<WorkflowId>,
    /// Human-readable title.
    pub title: String,
    /// Comprehensive high-level software engineering objective.
    pub objective: String,
    /// Current lifecycle state.
    pub state: MissionState,
    /// Current execution cycle index (0-indexed).
    pub cycle_index: u32,
    /// Configured resource and operational limits.
    pub budget: MissionBudget,
    /// Cumulative resource usage.
    pub budget_consumed: MissionBudgetConsumed,
    /// Real-time health assessment.
    pub health_status: MissionHealth,
    /// Verifiable physical stopping condition.
    pub stopping_condition: StoppingCondition,
    /// Latest verified repository commit SHA on disk.
    pub latest_verified_commit: Option<String>,
    /// Final outcome when mission reaches terminal state.
    pub final_outcome: Option<MissionOutcome>,
    /// Reason why human escalation was triggered if state is `NeedsHuman`.
    pub escalation_reason: Option<String>,
    /// Structured arbitrary metadata.
    pub metadata: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

impl Mission {
    /// Creates a new Mission in `Created` state.
    pub fn new(title: impl Into<String>, objective: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: MissionId::new(),
            workspace_id: None,
            active_workflow_id: None,
            title: title.into(),
            objective: objective.into(),
            state: MissionState::Created,
            cycle_index: 0,
            budget: MissionBudget::default(),
            budget_consumed: MissionBudgetConsumed::default(),
            health_status: MissionHealth::Healthy,
            stopping_condition: StoppingCondition::default(),
            latest_verified_commit: None,
            final_outcome: None,
            escalation_reason: None,
            metadata: serde_json::Value::Object(Default::default()),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_workspace_id(mut self, workspace_id: WorkspaceId) -> Self {
        self.workspace_id = Some(workspace_id);
        self
    }

    pub fn with_budget(mut self, budget: MissionBudget) -> Self {
        self.budget = budget;
        self
    }

    pub fn with_stopping_condition(mut self, condition: StoppingCondition) -> Self {
        self.stopping_condition = condition;
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn set_state(&mut self, state: MissionState) {
        self.state = state;
        self.updated_at = Utc::now();
    }
}

/// Durable checkpoint capturing the complete state necessary to resume a mission across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionCheckpoint {
    /// Unique checkpoint identifier.
    pub id: CheckpointId,
    /// Associated mission identifier.
    pub mission_id: MissionId,
    /// Cycle index at the time this checkpoint was recorded.
    pub cycle_index: u32,
    /// Active workflow identifier at this checkpoint.
    pub workflow_id: WorkflowId,
    /// Summary map of task IDs to their TaskState string.
    pub task_states_summary: serde_json::Value,
    /// Active process execution IDs at checkpoint time.
    pub active_executions: Vec<ExecutionId>,
    /// Tasks successfully verified in this mission.
    pub completed_tasks: Vec<TaskId>,
    /// Tasks still pending, in progress, or requiring replanning.
    pub unresolved_tasks: Vec<TaskId>,
    /// Budget consumption recorded at checkpoint time.
    pub budget_consumed: MissionBudgetConsumed,
    /// Latest verified Git commit SHA at checkpoint time.
    pub latest_verified_commit: Option<String>,
    /// Planner context and discovered requirements for continuation.
    pub planner_context: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl MissionCheckpoint {
    pub fn new(
        mission_id: MissionId,
        cycle_index: u32,
        workflow_id: WorkflowId,
        task_states_summary: serde_json::Value,
        budget_consumed: MissionBudgetConsumed,
    ) -> Self {
        Self {
            id: CheckpointId::new(),
            mission_id,
            cycle_index,
            workflow_id,
            task_states_summary,
            active_executions: Vec::new(),
            completed_tasks: Vec::new(),
            unresolved_tasks: Vec::new(),
            budget_consumed,
            latest_verified_commit: None,
            planner_context: serde_json::Value::Object(Default::default()),
            created_at: Utc::now(),
        }
    }
}

/// Execution cycle record tracking autonomous phase progression within a mission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionCycle {
    /// Unique cycle record identifier.
    pub id: String,
    /// Associated mission identifier.
    pub mission_id: MissionId,
    /// Cycle index (0-indexed).
    pub cycle_index: u32,
    /// Workflow executing this cycle.
    pub workflow_id: WorkflowId,
    /// Phase label (e.g. "investigate", "implement", "replan", "verify", "integrate").
    pub phase: String,
    /// Start timestamp.
    pub started_at: DateTime<Utc>,
    /// Completion timestamp if finished.
    pub completed_at: Option<DateTime<Utc>>,
    /// Summary outcome or diagnostics for this cycle.
    pub outcome: Option<String>,
    /// Number of tasks dynamically discovered and added during this cycle.
    pub discovered_tasks_count: u32,
}

impl MissionCycle {
    pub fn new(
        mission_id: MissionId,
        cycle_index: u32,
        workflow_id: WorkflowId,
        phase: impl Into<String>,
    ) -> Self {
        Self {
            id: format!("cycle_{}_{}", mission_id, cycle_index),
            mission_id,
            cycle_index,
            workflow_id,
            phase: phase.into(),
            started_at: Utc::now(),
            completed_at: None,
            outcome: None,
            discovered_tasks_count: 0,
        }
    }
}
