//! Failure diagnosis and recovery action selection.
//!
//! Analyzes execution errors and verification evidence to distinguish
//! transient failures from strategy changes, decompositions, and human escalation.

use plexis_core::{Execution, Task, Verification, VerificationVerdict};

/// Semantic categorization of why a task execution or verification failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureCategory {
    /// Transient error such as network timeout or provider unavailable.
    Transient(String),
    /// An expected file artifact was missing on disk.
    MissingArtifact(String),
    /// File artifact was present but acceptance criteria content was unmet.
    ContentCriteriaUnmet(String),
    /// A tool was blocked due to missing permissions or capability restrictions.
    PermissionDenied(String),
    /// The task is too complex or broad, failing repeatedly.
    ExcessiveComplexity(String),
    /// Execution attempts reached or exceeded maximum limit.
    AttemptsExhausted(String),
    /// Unknown or generic failure.
    Unknown(String),
}

/// Recovery action determined by the diagnoser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Retry the task with the same strategy and agent.
    RetrySameStrategy,
    /// Strategy change: retry with targeted diagnostic feedback injected into context.
    RetryWithFeedback(String),
    /// Strategy change: decompose the failing task into smaller subtasks.
    DecomposeTask,
    /// Strategy change: reassign to an agent with different capabilities.
    ReassignAgent(Vec<String>),
    /// Escalate to human governance (transition to `NeedsHuman`).
    EscalateHuman(String),
}

/// Diagnostic outcome containing the categorized failure and recommended recovery action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryRecommendation {
    /// Categorized diagnosis.
    pub category: FailureCategory,
    /// Detailed diagnostic rationale.
    pub rationale: String,
    /// Recommended recovery pathway.
    pub action: RecoveryAction,
}

/// Intelligent diagnoser for Plexis task failures.
pub struct FailureDiagnoser;

impl FailureDiagnoser {
    /// Diagnoses task failure using task state, execution history, and independent verification evidence.
    pub fn diagnose(
        task: &Task,
        execution: Option<&Execution>,
        verification: Option<&Verification>,
    ) -> RecoveryRecommendation {
        // 1. Check if max attempts are exhausted
        if task.attempts >= task.max_attempts {
            let reason = format!(
                "Task '{}' reached max attempts ({}/{}) without successful verification",
                task.id, task.attempts, task.max_attempts
            );
            return RecoveryRecommendation {
                category: FailureCategory::AttemptsExhausted(reason.clone()),
                rationale: reason.clone(),
                action: RecoveryAction::EscalateHuman(reason),
            };
        }

        // 2. Check independent verification failure evidence
        if let Some(verif) = verification {
            if verif.verdict == VerificationVerdict::Failed {
                let fail_msg = verif
                    .failure_reason
                    .clone()
                    .unwrap_or_else(|| "Criteria not satisfied".to_string());

                if fail_msg.to_lowercase().contains("missing file")
                    || fail_msg.to_lowercase().contains("file not found")
                {
                    let feedback = format!(
                        "Independent verification failed: {}. Ensure the required files are created in the workspace.",
                        fail_msg
                    );
                    return RecoveryRecommendation {
                        category: FailureCategory::MissingArtifact(fail_msg),
                        rationale: feedback.clone(),
                        action: RecoveryAction::RetryWithFeedback(feedback),
                    };
                }

                if fail_msg.to_lowercase().contains("content mismatch")
                    || fail_msg.to_lowercase().contains("pattern not found")
                {
                    let feedback = format!(
                        "Independent verification failed: {}. Review criteria and update artifact contents.",
                        fail_msg
                    );
                    return RecoveryRecommendation {
                        category: FailureCategory::ContentCriteriaUnmet(fail_msg),
                        rationale: feedback.clone(),
                        action: RecoveryAction::RetryWithFeedback(feedback),
                    };
                }

                // If failed more than once, recommend decomposition
                if task.attempts >= 2 {
                    return RecoveryRecommendation {
                        category: FailureCategory::ExcessiveComplexity(fail_msg.clone()),
                        rationale: format!(
                            "Task '{}' failed multiple verification attempts; recommend decomposition",
                            task.id
                        ),
                        action: RecoveryAction::DecomposeTask,
                    };
                }

                return RecoveryRecommendation {
                    category: FailureCategory::ContentCriteriaUnmet(fail_msg.clone()),
                    rationale: fail_msg.clone(),
                    action: RecoveryAction::RetryWithFeedback(format!(
                        "Verification failed: {}. Please fix.",
                        fail_msg
                    )),
                };
            }
        }

        // 3. Check execution errors
        if let Some(exec) = execution {
            if let Some(err_msg) = &exec.error_message {
                let lower = err_msg.to_lowercase();

                if lower.contains("permission denied") || lower.contains("sandbox violation") {
                    return RecoveryRecommendation {
                        category: FailureCategory::PermissionDenied(err_msg.clone()),
                        rationale: format!("Security violation or missing capability: {}", err_msg),
                        action: RecoveryAction::EscalateHuman(format!(
                            "Security boundary hold: {}",
                            err_msg
                        )),
                    };
                }

                if lower.contains("timeout")
                    || lower.contains("unavailable")
                    || lower.contains("503")
                {
                    return RecoveryRecommendation {
                        category: FailureCategory::Transient(err_msg.clone()),
                        rationale: format!("Transient error encountered: {}", err_msg),
                        action: RecoveryAction::RetrySameStrategy,
                    };
                }

                if task.attempts >= 2 {
                    return RecoveryRecommendation {
                        category: FailureCategory::ExcessiveComplexity(err_msg.clone()),
                        rationale: format!("Repeated execution error: {}", err_msg),
                        action: RecoveryAction::DecomposeTask,
                    };
                }

                return RecoveryRecommendation {
                    category: FailureCategory::Unknown(err_msg.clone()),
                    rationale: format!("Execution failed: {}", err_msg),
                    action: RecoveryAction::RetryWithFeedback(format!(
                        "Previous run failed with error: {}. Adjust implementation.",
                        err_msg
                    )),
                };
            }
        }

        // Default fallback
        RecoveryRecommendation {
            category: FailureCategory::Unknown("Unspecified failure".into()),
            rationale: "No specific error or verification evidence provided".into(),
            action: RecoveryAction::RetrySameStrategy,
        }
    }
}
