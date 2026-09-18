use chrono::Utc;
use std::sync::Arc;
use tracing::{info, warn};

use plexis_core::ids::{ExecutionId, RecoveryId, TaskId, WorkflowId};
use plexis_core::{RecoveryRecord, RecoveryResult, Task};
use plexis_storage::traits::RecoveryStore;

use crate::error::RuntimeError;

/// Recovery action determined by the RecoveryController.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Retry with backoff delay.
    RetryWithBackoff { delay_secs: u64 },
    /// Mutate strategy (e.g. prompt adjustment, tool substitution, environment repair).
    MutateStrategy {
        strategy: String,
        adjustment: String,
        version: u32,
    },
    /// Reassign task to a different agent or higher reasoning tier.
    ReassignAgent { suggested_role: String },
    /// Replanning required due to logical deadlock or unresolvable assumption.
    ReplanWorkflow { reason: String },
    /// Fatal unrecoverable failure; fail fast and escalate to human.
    EscalateFatal { reason: String },
}

/// Systematic failure recovery controller with strategy mutation and infinite loop detection.
pub struct RecoveryController<S: RecoveryStore + 'static> {
    store: Arc<S>,
    max_recovery_attempts: u32,
}

impl<S: RecoveryStore + 'static> RecoveryController<S> {
    pub fn new(store: Arc<S>, max_recovery_attempts: u32) -> Self {
        Self {
            store,
            max_recovery_attempts,
        }
    }

    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    /// Evaluates a task failure, diagnoses the root cause, checks for repeating failure patterns,
    /// decides a mutated recovery action, and persists a durable RecoveryRecord.
    pub async fn diagnose_and_recover(
        &self,
        task: &Task,
        workflow_id: &WorkflowId,
        execution_id: Option<&ExecutionId>,
        attempt: u32,
        failure_reason: &str,
    ) -> Result<(RecoveryAction, RecoveryRecord), RuntimeError> {
        // 1. Fetch historical recovery records for this task
        let past_records = self
            .store
            .list_recovery_records_by_task(&task.id)
            .await
            .map_err(RuntimeError::Storage)?;

        // 2. Infinite loop / repetition check
        if attempt >= self.max_recovery_attempts {
            let reason = format!(
                "Task '{}' exceeded maximum recovery attempts ({}/{})",
                task.id, attempt, self.max_recovery_attempts
            );
            let action = RecoveryAction::EscalateFatal {
                reason: reason.clone(),
            };
            let record = self
                .record_decision(
                    task.id,
                    *workflow_id,
                    execution_id.cloned(),
                    attempt,
                    "fatal_exhaustion".to_string(),
                    1,
                    failure_reason.to_string(),
                    serde_json::json!({ "exceeded_max_attempts": true }),
                    "escalate_fatal".to_string(),
                    Some(reason),
                    RecoveryResult::Escalated,
                )
                .await?;
            return Ok((action, record));
        }

        // Check if the same failure reason repeated consecutively
        let consecutive_identical = past_records
            .iter()
            .rev()
            .take_while(|r| r.failure_reason == failure_reason)
            .count();

        if consecutive_identical >= 2 {
            warn!(
                task_id = %task.id,
                consecutive = consecutive_identical,
                reason = failure_reason,
                "Detected repeating failure pattern; triggering replanning"
            );
            let replan_reason = format!(
                "Repeated failure pattern (failed {} times with: '{}')",
                consecutive_identical + 1,
                failure_reason
            );
            let action = RecoveryAction::ReplanWorkflow {
                reason: replan_reason.clone(),
            };
            let record = self
                .record_decision(
                    task.id,
                    *workflow_id,
                    execution_id.cloned(),
                    attempt,
                    "replan_loop_detected".to_string(),
                    (consecutive_identical + 1) as u32,
                    failure_reason.to_string(),
                    serde_json::json!({ "loop_detected": true, "consecutive": consecutive_identical }),
                    "replan_workflow".to_string(),
                    Some(replan_reason),
                    RecoveryResult::Escalated,
                )
                .await?;
            return Ok((action, record));
        }

        // 3. Strategy classification and mutation based on failure diagnosis
        let lower = failure_reason.to_lowercase();

        let (action, strategy, version) = if lower.contains("rate limited")
            || lower.contains("connection reset")
            || lower.contains("gateway timeout")
        {
            // Transient network / provider error
            let delay = (attempt as u64 + 1) * 2;
            (
                RecoveryAction::RetryWithBackoff { delay_secs: delay },
                "transient_backoff".to_string(),
                1,
            )
        } else if lower.contains("permission denied") || lower.contains("sandbox") {
            // Security or sandbox barrier
            (
                RecoveryAction::EscalateFatal {
                    reason: "Sandbox security policy violation cannot be bypassed automatically"
                        .to_string(),
                },
                "security_escalation".to_string(),
                1,
            )
        } else if lower.contains("timeout") || lower.contains("timed out") {
            // Timeout error: extend execution budget and mutate strategy
            let current_version = past_records
                .iter()
                .filter(|r| r.strategy == "timeout_adaptation")
                .map(|r| r.strategy_version)
                .max()
                .unwrap_or(0)
                + 1;

            (
                RecoveryAction::MutateStrategy {
                    strategy: "timeout_adaptation".to_string(),
                    adjustment: "Double execution timeout and refine review instructions"
                        .to_string(),
                    version: current_version,
                },
                "timeout_adaptation".to_string(),
                current_version,
            )
        } else if lower.contains("tool")
            || lower.contains("command failed")
            || lower.contains("exit code")
        {
            // Tool execution error: mutate strategy to prompt adjustment / alternative tool
            let current_version = past_records
                .iter()
                .filter(|r| r.strategy == "tool_adaptation")
                .map(|r| r.strategy_version)
                .max()
                .unwrap_or(0)
                + 1;

            (
                RecoveryAction::MutateStrategy {
                    strategy: "tool_adaptation".to_string(),
                    adjustment:
                        "Inspect command error output, use diagnostic tools, and revise parameters"
                            .to_string(),
                    version: current_version,
                },
                "tool_adaptation".to_string(),
                current_version,
            )
        } else {
            // General reasoning or verification failure
            let current_version = past_records
                .iter()
                .filter(|r| r.strategy == "prompt_refinement")
                .map(|r| r.strategy_version)
                .max()
                .unwrap_or(0)
                + 1;

            (
                RecoveryAction::MutateStrategy {
                    strategy: "prompt_refinement".to_string(),
                    adjustment: "Deconstruct acceptance criteria step-by-step and verify state incrementally"
                        .to_string(),
                    version: current_version,
                },
                "prompt_refinement".to_string(),
                current_version,
            )
        };

        info!(
            task_id = %task.id,
            attempt,
            strategy = %strategy,
            version,
            "Determined recovery action"
        );

        let action_str = format!("{:?}", action);
        let record = self
            .record_decision(
                task.id,
                *workflow_id,
                execution_id.cloned(),
                attempt,
                strategy,
                version,
                failure_reason.to_string(),
                serde_json::json!({ "failure_text": failure_reason }),
                action_str,
                None,
                RecoveryResult::InProgress,
            )
            .await?;

        Ok((action, record))
    }

    /// Records that a recovery action succeeded.
    pub async fn mark_recovered(&self, recovery_id: &RecoveryId) -> Result<(), RuntimeError> {
        if let Some(mut record) = self
            .store
            .get_recovery_record(recovery_id)
            .await
            .map_err(RuntimeError::Storage)?
        {
            record.result = RecoveryResult::Succeeded;
            record.updated_at = Utc::now();
            self.store
                .update_recovery_record(&record)
                .await
                .map_err(RuntimeError::Storage)?;
            info!(recovery_id = %recovery_id, "Recovery marked succeeded");
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_decision(
        &self,
        task_id: TaskId,
        workflow_id: WorkflowId,
        execution_id: Option<ExecutionId>,
        attempt: u32,
        strategy: String,
        strategy_version: u32,
        failure_reason: String,
        diagnosis: serde_json::Value,
        recovery_action: String,
        action_reason: Option<String>,
        result: RecoveryResult,
    ) -> Result<RecoveryRecord, RuntimeError> {
        let now = Utc::now();
        let record = RecoveryRecord {
            id: RecoveryId::new(),
            task_id,
            workflow_id,
            execution_id,
            attempt,
            strategy,
            strategy_version,
            failure_reason,
            diagnosis,
            recovery_action,
            action_reason,
            result,
            created_at: now,
            updated_at: now,
        };

        self.store
            .save_recovery_record(&record)
            .await
            .map_err(RuntimeError::Storage)?;

        Ok(record)
    }
}
