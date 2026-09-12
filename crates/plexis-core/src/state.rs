//! Explicit semantic state models and validated transitions for Plexis.
//!
//! State transitions must be deterministic and legal. LLMs are never permitted
//! to dictate or bypass state-transition invariants.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Error returned when an invalid state transition is attempted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("illegal state transition from {from} to {to}: {reason}")]
pub struct StateTransitionError {
    pub from: String,
    pub to: String,
    pub reason: &'static str,
}

/// Task lifecycle state.
///
/// Task state and execution state are strictly distinct. A task can be `Running`
/// while the executing agent is `WaitingForInput`, or a task can be `AwaitingVerification`
/// while the producer agent has become `Idle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Task created but dependencies or requirements are not yet ready.
    Backlog,
    /// Dependencies are met; task is eligible for assignment and scheduling.
    Ready,
    /// Task has been assigned to an agent under an active lease.
    Assigned,
    /// Active execution is in progress.
    Running,
    /// Agent finished work; independent verifier must inspect before completion.
    AwaitingVerification,
    /// Independent verifier confirmed success (terminal success state).
    Verified,
    /// Task is blocked by unmet conditions, external holds, or policy.
    Blocked,
    /// Human approval or intervention is required.
    NeedsHuman,
    /// Task is administratively paused.
    Paused,
    /// Execution or verification failed.
    Failed,
    /// Task is being retried with previous failure evidence.
    Retrying,
    /// Task is quarantined due to repeated failures or runaway behaviour.
    Quarantined,
    /// Task was administratively cancelled.
    Cancelled,
    /// Task was discarded as no longer necessary.
    Discarded,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Backlog => "backlog",
            TaskState::Ready => "ready",
            TaskState::Assigned => "assigned",
            TaskState::Running => "running",
            TaskState::AwaitingVerification => "awaiting_verification",
            TaskState::Verified => "verified",
            TaskState::Blocked => "blocked",
            TaskState::NeedsHuman => "needs_human",
            TaskState::Paused => "paused",
            TaskState::Failed => "failed",
            TaskState::Retrying => "retrying",
            TaskState::Quarantined => "quarantined",
            TaskState::Cancelled => "cancelled",
            TaskState::Discarded => "discarded",
        }
    }

    /// Checks whether a transition from `self` to `next` is allowed by domain rules.
    pub fn can_transition_to(&self, next: &TaskState) -> bool {
        if self == next {
            return true;
        }

        match self {
            TaskState::Backlog => matches!(
                next,
                TaskState::Ready
                    | TaskState::Blocked
                    | TaskState::Paused
                    | TaskState::Cancelled
                    | TaskState::Discarded
            ),
            TaskState::Ready => matches!(
                next,
                TaskState::Assigned
                    | TaskState::Blocked
                    | TaskState::Paused
                    | TaskState::NeedsHuman
                    | TaskState::Cancelled
                    | TaskState::Discarded
            ),
            TaskState::Assigned => matches!(
                next,
                TaskState::Running
                    | TaskState::Ready // lease expired or released
                    | TaskState::NeedsHuman
                    | TaskState::Failed
                    | TaskState::Paused
                    | TaskState::Cancelled
            ),
            TaskState::Running => matches!(
                next,
                TaskState::AwaitingVerification
                    | TaskState::Paused
                    | TaskState::NeedsHuman
                    | TaskState::Failed
                    | TaskState::Retrying
                    | TaskState::Blocked
                    | TaskState::Cancelled
            ),
            TaskState::AwaitingVerification => matches!(
                next,
                TaskState::Verified
                    | TaskState::Failed
                    | TaskState::Retrying
                    | TaskState::NeedsHuman
                    | TaskState::Running // additional work requested by verifier
                    | TaskState::Cancelled
            ),
            TaskState::Verified => {
                // Verified is generally terminal, but can be retried if subsequent regression occurs.
                matches!(next, TaskState::Retrying | TaskState::Discarded)
            }
            TaskState::Blocked => matches!(
                next,
                TaskState::Ready | TaskState::Backlog | TaskState::Cancelled | TaskState::Discarded
            ),
            TaskState::NeedsHuman => matches!(
                next,
                TaskState::Ready
                    | TaskState::Running
                    | TaskState::AwaitingVerification
                    | TaskState::Failed
                    | TaskState::Quarantined
                    | TaskState::Cancelled
                    | TaskState::Discarded
            ),
            TaskState::Paused => matches!(
                next,
                TaskState::Backlog
                    | TaskState::Ready
                    | TaskState::Running
                    | TaskState::Cancelled
                    | TaskState::Discarded
            ),
            TaskState::Failed => matches!(
                next,
                TaskState::Retrying
                    | TaskState::Ready
                    | TaskState::NeedsHuman
                    | TaskState::Quarantined
                    | TaskState::Cancelled
                    | TaskState::Discarded
            ),
            TaskState::Retrying => matches!(
                next,
                TaskState::Ready
                    | TaskState::Running
                    | TaskState::Failed
                    | TaskState::Quarantined
                    | TaskState::Cancelled
            ),
            TaskState::Quarantined => matches!(
                next,
                TaskState::Backlog
                    | TaskState::Ready
                    | TaskState::NeedsHuman
                    | TaskState::Cancelled
                    | TaskState::Discarded
            ),
            TaskState::Cancelled | TaskState::Discarded => false,
        }
    }

    /// Attempts to execute a transition to `next`.
    pub fn transition_to(&mut self, next: TaskState) -> Result<(), StateTransitionError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(StateTransitionError {
                from: self.as_str().to_string(),
                to: next.as_str().to_string(),
                reason: "transition disallowed by task lifecycle rules",
            })
        }
    }

    /// Indicates whether the task has reached a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Verified | TaskState::Cancelled | TaskState::Discarded
        )
    }

    /// Indicates whether the task is actively executing or assigned.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TaskState::Assigned | TaskState::Running | TaskState::AwaitingVerification
        )
    }

    /// Indicates whether the task is eligible to run if dependencies are satisfied.
    pub fn is_runnable_candidate(&self) -> bool {
        matches!(self, TaskState::Ready | TaskState::Backlog)
    }
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for TaskState {
    type Err = StateTransitionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().trim_matches('"') {
            "backlog" => Ok(TaskState::Backlog),
            "ready" => Ok(TaskState::Ready),
            "assigned" => Ok(TaskState::Assigned),
            "running" => Ok(TaskState::Running),
            "awaiting_verification" => Ok(TaskState::AwaitingVerification),
            "verified" => Ok(TaskState::Verified),
            "blocked" => Ok(TaskState::Blocked),
            "needs_human" => Ok(TaskState::NeedsHuman),
            "paused" => Ok(TaskState::Paused),
            "failed" => Ok(TaskState::Failed),
            "retrying" => Ok(TaskState::Retrying),
            "quarantined" => Ok(TaskState::Quarantined),
            "cancelled" => Ok(TaskState::Cancelled),
            "discarded" => Ok(TaskState::Discarded),
            other => Err(StateTransitionError {
                from: other.to_string(),
                to: "".to_string(),
                reason: "unknown task state string",
            }),
        }
    }
}

/// Agent lifecycle and activity state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Agent is idle and available for new tasks.
    Idle,
    /// Agent is actively executing an assignment.
    Busy,
    /// Agent is awaiting tool output, human feedback, or peer message.
    WaitingForInput,
    /// Agent execution is administratively paused.
    Paused,
    /// Agent is permanently terminated.
    Terminated,
    /// Agent encountered a fatal error or unhandled crash.
    Failed,
}

impl AgentState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentState::Idle => "idle",
            AgentState::Busy => "busy",
            AgentState::WaitingForInput => "waiting_for_input",
            AgentState::Paused => "paused",
            AgentState::Terminated => "terminated",
            AgentState::Failed => "failed",
        }
    }

    pub fn can_accept_work(&self) -> bool {
        matches!(self, AgentState::Idle)
    }

    pub fn can_transition_to(&self, next: &AgentState) -> bool {
        if self == next {
            return true;
        }

        match self {
            AgentState::Idle => matches!(
                next,
                AgentState::Busy | AgentState::Paused | AgentState::Terminated | AgentState::Failed
            ),
            AgentState::Busy => matches!(
                next,
                AgentState::Idle
                    | AgentState::WaitingForInput
                    | AgentState::Paused
                    | AgentState::Failed
                    | AgentState::Terminated
            ),
            AgentState::WaitingForInput => matches!(
                next,
                AgentState::Busy
                    | AgentState::Idle
                    | AgentState::Paused
                    | AgentState::Failed
                    | AgentState::Terminated
            ),
            AgentState::Paused => matches!(
                next,
                AgentState::Idle
                    | AgentState::Busy
                    | AgentState::WaitingForInput
                    | AgentState::Terminated
            ),
            AgentState::Failed => matches!(next, AgentState::Idle | AgentState::Terminated),
            AgentState::Terminated => false,
        }
    }

    pub fn transition_to(&mut self, next: AgentState) -> Result<(), StateTransitionError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(StateTransitionError {
                from: self.as_str().to_string(),
                to: next.as_str().to_string(),
                reason: "transition disallowed by agent lifecycle rules",
            })
        }
    }
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for AgentState {
    type Err = StateTransitionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().trim_matches('"') {
            "idle" => Ok(AgentState::Idle),
            "busy" => Ok(AgentState::Busy),
            "waiting_for_input" => Ok(AgentState::WaitingForInput),
            "paused" => Ok(AgentState::Paused),
            "terminated" => Ok(AgentState::Terminated),
            "failed" => Ok(AgentState::Failed),
            other => Err(StateTransitionError {
                from: other.to_string(),
                to: "".to_string(),
                reason: "unknown agent state string",
            }),
        }
    }
}

/// Workflow overall lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    /// Workflow is in draft / planning phase.
    Draft,
    /// Workflow is actively executing.
    Active,
    /// Workflow execution is paused.
    Paused,
    /// All tasks completed and verified.
    Completed,
    /// Workflow failed unrecoverably.
    Failed,
    /// Workflow was cancelled.
    Cancelled,
}

impl WorkflowState {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorkflowState::Draft => "draft",
            WorkflowState::Active => "active",
            WorkflowState::Paused => "paused",
            WorkflowState::Completed => "completed",
            WorkflowState::Failed => "failed",
            WorkflowState::Cancelled => "cancelled",
        }
    }

    pub fn can_transition_to(&self, next: &WorkflowState) -> bool {
        if self == next {
            return true;
        }

        match self {
            WorkflowState::Draft => {
                matches!(next, WorkflowState::Active | WorkflowState::Cancelled)
            }
            WorkflowState::Active => matches!(
                next,
                WorkflowState::Paused
                    | WorkflowState::Completed
                    | WorkflowState::Failed
                    | WorkflowState::Cancelled
            ),
            WorkflowState::Paused => {
                matches!(next, WorkflowState::Active | WorkflowState::Cancelled)
            }
            WorkflowState::Completed | WorkflowState::Failed | WorkflowState::Cancelled => false,
        }
    }

    pub fn transition_to(&mut self, next: WorkflowState) -> Result<(), StateTransitionError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(StateTransitionError {
                from: self.as_str().to_string(),
                to: next.as_str().to_string(),
                reason: "transition disallowed by workflow lifecycle rules",
            })
        }
    }
}

impl fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for WorkflowState {
    type Err = StateTransitionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().trim_matches('"') {
            "draft" => Ok(WorkflowState::Draft),
            "active" => Ok(WorkflowState::Active),
            "paused" => Ok(WorkflowState::Paused),
            "completed" => Ok(WorkflowState::Completed),
            "failed" => Ok(WorkflowState::Failed),
            "cancelled" => Ok(WorkflowState::Cancelled),
            other => Err(StateTransitionError {
                from: other.to_string(),
                to: "".to_string(),
                reason: "unknown workflow state string",
            }),
        }
    }
}

/// Execution attempt lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    /// Queued / pending start.
    Pending,
    /// Running live process.
    Running,
    /// Completed successfully.
    Completed,
    /// Failed with error.
    Failed,
    /// Exceeded maximum execution duration.
    TimedOut,
    /// Cancelled before completion.
    Cancelled,
}

impl ExecutionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionState::Pending => "pending",
            ExecutionState::Running => "running",
            ExecutionState::Completed => "completed",
            ExecutionState::Failed => "failed",
            ExecutionState::TimedOut => "timed_out",
            ExecutionState::Cancelled => "cancelled",
        }
    }

    pub fn can_transition_to(&self, next: &ExecutionState) -> bool {
        if self == next {
            return true;
        }

        match self {
            ExecutionState::Pending => {
                matches!(next, ExecutionState::Running | ExecutionState::Cancelled)
            }
            ExecutionState::Running => matches!(
                next,
                ExecutionState::Completed
                    | ExecutionState::Failed
                    | ExecutionState::TimedOut
                    | ExecutionState::Cancelled
            ),
            ExecutionState::Completed
            | ExecutionState::Failed
            | ExecutionState::TimedOut
            | ExecutionState::Cancelled => false,
        }
    }

    pub fn transition_to(&mut self, next: ExecutionState) -> Result<(), StateTransitionError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(StateTransitionError {
                from: self.as_str().to_string(),
                to: next.as_str().to_string(),
                reason: "transition disallowed by execution lifecycle rules",
            })
        }
    }
}

impl fmt::Display for ExecutionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ExecutionState {
    type Err = StateTransitionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().trim_matches('"') {
            "pending" => Ok(ExecutionState::Pending),
            "running" => Ok(ExecutionState::Running),
            "completed" => Ok(ExecutionState::Completed),
            "failed" => Ok(ExecutionState::Failed),
            "timed_out" => Ok(ExecutionState::TimedOut),
            "cancelled" => Ok(ExecutionState::Cancelled),
            other => Err(StateTransitionError {
                from: other.to_string(),
                to: "".to_string(),
                reason: "unknown execution state string",
            }),
        }
    }
}

/// Command lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandState {
    /// Command is queued for delivery.
    Queued,
    /// Command was dispatched to runtime or target agent.
    Dispatched,
    /// Command was acknowledged as received by runtime.
    Delivered,
    /// Command finished execution with confirmation.
    Confirmed,
    /// Command execution failed.
    Failed,
    /// Command is awaiting retry after failure.
    Retrying,
    /// Command reached retry threshold and moved to dead-letter status.
    DeadLettered,
    /// Command was administratively cancelled.
    Cancelled,
}

impl CommandState {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandState::Queued => "queued",
            CommandState::Dispatched => "dispatched",
            CommandState::Delivered => "delivered",
            CommandState::Confirmed => "confirmed",
            CommandState::Failed => "failed",
            CommandState::Retrying => "retrying",
            CommandState::DeadLettered => "dead_lettered",
            CommandState::Cancelled => "cancelled",
        }
    }

    pub fn can_transition_to(&self, next: &CommandState) -> bool {
        if self == next {
            return true;
        }

        match self {
            CommandState::Queued => {
                matches!(next, CommandState::Dispatched | CommandState::Cancelled)
            }
            CommandState::Dispatched => matches!(
                next,
                CommandState::Delivered
                    | CommandState::Confirmed
                    | CommandState::Failed
                    | CommandState::Retrying
                    | CommandState::Cancelled
            ),
            CommandState::Delivered => matches!(
                next,
                CommandState::Confirmed
                    | CommandState::Failed
                    | CommandState::Retrying
                    | CommandState::Cancelled
            ),
            CommandState::Retrying => matches!(
                next,
                CommandState::Queued
                    | CommandState::Dispatched
                    | CommandState::DeadLettered
                    | CommandState::Cancelled
            ),
            CommandState::Failed => {
                matches!(
                    next,
                    CommandState::Retrying | CommandState::DeadLettered | CommandState::Cancelled
                )
            }
            CommandState::Confirmed | CommandState::DeadLettered | CommandState::Cancelled => false,
        }
    }

    pub fn transition_to(&mut self, next: CommandState) -> Result<(), StateTransitionError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(StateTransitionError {
                from: self.as_str().to_string(),
                to: next.as_str().to_string(),
                reason: "transition disallowed by command lifecycle rules",
            })
        }
    }
}

impl fmt::Display for CommandState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CommandState {
    type Err = StateTransitionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().trim_matches('"') {
            "queued" => Ok(CommandState::Queued),
            "dispatched" => Ok(CommandState::Dispatched),
            "delivered" => Ok(CommandState::Delivered),
            "confirmed" => Ok(CommandState::Confirmed),
            "failed" => Ok(CommandState::Failed),
            "retrying" => Ok(CommandState::Retrying),
            "dead_lettered" => Ok(CommandState::DeadLettered),
            "cancelled" => Ok(CommandState::Cancelled),
            other => Err(StateTransitionError {
                from: other.to_string(),
                to: "".to_string(),
                reason: "unknown command state string",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_task_state_transitions() {
        let mut state = TaskState::Backlog;
        assert!(state.transition_to(TaskState::Ready).is_ok());
        assert!(state.transition_to(TaskState::Assigned).is_ok());
        assert!(state.transition_to(TaskState::Running).is_ok());
        assert!(state.transition_to(TaskState::AwaitingVerification).is_ok());
        assert!(state.transition_to(TaskState::Verified).is_ok());
        assert!(state.is_terminal());
    }

    #[test]
    fn test_invalid_task_state_transitions() {
        let mut state = TaskState::Backlog;
        assert!(state.transition_to(TaskState::Verified).is_err());

        let mut cancelled = TaskState::Cancelled;
        assert!(cancelled.transition_to(TaskState::Ready).is_err());
    }

    #[test]
    fn test_task_failure_and_retry_transitions() {
        let mut state = TaskState::Running;
        assert!(state.transition_to(TaskState::Failed).is_ok());
        assert!(state.transition_to(TaskState::Retrying).is_ok());
        assert!(state.transition_to(TaskState::Ready).is_ok());
    }

    #[test]
    fn test_human_intervention_transitions() {
        let mut state = TaskState::Running;
        assert!(state.transition_to(TaskState::NeedsHuman).is_ok());
        // Human can decide to fail or resume task
        let mut fail_path = state;
        assert!(fail_path.transition_to(TaskState::Failed).is_ok());
        let mut resume_path = state;
        assert!(resume_path.transition_to(TaskState::Ready).is_ok());
    }

    #[test]
    fn test_execution_state_transitions() {
        let mut state = ExecutionState::Pending;
        assert!(state.transition_to(ExecutionState::Running).is_ok());
        assert!(state.transition_to(ExecutionState::Completed).is_ok());
        // Completed cannot transition to running
        assert!(state.transition_to(ExecutionState::Running).is_err());
    }

    #[test]
    fn test_agent_state_transitions() {
        let mut state = AgentState::Idle;
        assert!(state.transition_to(AgentState::Busy).is_ok());
        assert!(state.transition_to(AgentState::WaitingForInput).is_ok());
        assert!(state.transition_to(AgentState::Idle).is_ok());
        assert!(state.transition_to(AgentState::Terminated).is_ok());
        assert!(state.transition_to(AgentState::Idle).is_err());
    }

    #[test]
    fn test_workflow_state_transitions() {
        let mut state = WorkflowState::Draft;
        assert!(state.transition_to(WorkflowState::Active).is_ok());
        assert!(state.transition_to(WorkflowState::Completed).is_ok());
        assert!(state.transition_to(WorkflowState::Active).is_err());
    }

    #[test]
    fn test_command_state_transitions() {
        let mut state = CommandState::Queued;
        assert!(state.transition_to(CommandState::Dispatched).is_ok());
        assert!(state.transition_to(CommandState::Failed).is_ok());
        assert!(state.transition_to(CommandState::Retrying).is_ok());
        assert!(state.transition_to(CommandState::Queued).is_ok());
    }

    #[test]
    fn test_state_string_roundtrip() {
        for s in [
            TaskState::Backlog,
            TaskState::Ready,
            TaskState::Assigned,
            TaskState::Running,
            TaskState::AwaitingVerification,
            TaskState::Verified,
        ] {
            let str_repr = s.as_str();
            let parsed: TaskState = str_repr.parse().unwrap();
            assert_eq!(s, parsed);
        }
    }
}
