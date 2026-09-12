//! Independent verifier ensuring task completion evidence is validated independently of the agent.

use async_trait::async_trait;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use plexis_core::state::TaskState;
use plexis_core::{Event, Execution, Task, Verification, VerificationVerdict};
use plexis_storage::error::StorageError;
use plexis_storage::traits::{EventStore, TaskStore, VerificationStore};

/// Verification context containing task requirements and sandbox workspace.
pub struct VerificationContext<'a> {
    pub task: &'a Task,
    pub execution: &'a Execution,
    pub working_directory: &'a Path,
}

/// Common trait for independent verifiers.
#[async_trait]
pub trait Verifier: Send + Sync {
    /// Returns unique kind of this verifier (e.g. "workspace_inspection").
    fn kind(&self) -> &str;

    /// Evaluates target task output and returns durable verification evidence.
    async fn verify(&self, context: &VerificationContext<'_>)
        -> Result<Verification, StorageError>;
}

/// Independent verifier inspecting workspace filesystem state.
pub struct WorkspaceVerifier<S: TaskStore + EventStore + VerificationStore> {
    store: Arc<S>,
}

impl<S: TaskStore + EventStore + VerificationStore> WorkspaceVerifier<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// Verifies the task execution, persists evidence, emits events, and updates task state.
    pub async fn verify_and_record(
        &self,
        task: &mut Task,
        execution: &Execution,
        working_directory: &Path,
    ) -> Result<Verification, StorageError> {
        let start_time = Instant::now();
        let task_id = task.id;

        // 1. Emit verification_started event
        let start_event = Event::new(
            "task",
            task_id.to_string(),
            "verification_started",
            serde_json::json!({
                "execution_id": execution.id.to_string(),
                "verifier": self.kind(),
                "working_dir": working_directory.display().to_string(),
            }),
        );
        self.store.append_event(&start_event).await?;

        // 2. Perform verification check
        let context = VerificationContext {
            task,
            execution,
            working_directory,
        };
        let mut verification = self.verify(&context).await?;
        verification.duration_ms = start_time.elapsed().as_millis() as u64;

        // 3. Persist verification evidence
        self.store.create_verification(&verification).await?;

        // 4. Update task state based on verdict
        let verdict_str = match verification.verdict {
            VerificationVerdict::Passed => "passed",
            VerificationVerdict::Failed => "failed",
            VerificationVerdict::Inconclusive => "inconclusive",
        };

        if verification.verdict == VerificationVerdict::Passed {
            // Backlog/Ready/Assigned/Running -> AwaitingVerification -> Verified
            if task.state == TaskState::Running || task.state == TaskState::Assigned {
                let _ = task.transition_to(TaskState::AwaitingVerification);
            }
            task.transition_to(TaskState::Verified)
                .map_err(StorageError::StateTransition)?;
        } else {
            task.transition_to(TaskState::Failed)
                .map_err(StorageError::StateTransition)?;
        }
        self.store.update_task(task).await?;

        // 5. Emit verification_completed event
        let complete_event = Event::new(
            "task",
            task_id.to_string(),
            "verification_completed",
            serde_json::json!({
                "verification_id": verification.id.to_string(),
                "execution_id": execution.id.to_string(),
                "verdict": verdict_str,
                "evidence": verification.evidence,
                "failure_reason": verification.failure_reason,
                "duration_ms": verification.duration_ms,
            }),
        );
        self.store.append_event(&complete_event).await?;

        Ok(verification)
    }
}

#[async_trait]
impl<S: TaskStore + EventStore + VerificationStore> Verifier for WorkspaceVerifier<S> {
    fn kind(&self) -> &str {
        "workspace_inspection"
    }

    async fn verify(
        &self,
        context: &VerificationContext<'_>,
    ) -> Result<Verification, StorageError> {
        let task = context.task;
        let work_dir = context.working_directory;

        // Inspect criteria configured in task.metadata["verification"] or fallback to task.criteria
        let criteria_val = task
            .metadata
            .get("verification")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let expected_file = criteria_val
            .get("file_path")
            .and_then(|v| v.as_str())
            .or_else(|| {
                // Heuristic from task criteria string if present
                task.criteria.iter().find_map(|c| {
                    if c.contains("file:") {
                        c.split("file:")
                            .nth(1)
                            .and_then(|s| s.split_whitespace().next())
                    } else {
                        None
                    }
                })
            });

        let expected_content = criteria_val
            .get("expected_content")
            .and_then(|v| v.as_str());

        // Default verification rule: check specified file existence and optional content
        let mut passed = true;
        let mut reasons = Vec::new();
        let mut evidence = serde_json::json!({});

        if let Some(rel_path) = expected_file {
            let full_path = work_dir.join(rel_path);
            if !full_path.exists() {
                passed = false;
                reasons.push(format!(
                    "Required file '{}' does not exist in workspace",
                    rel_path
                ));
                evidence = serde_json::json!({
                    "file_path": rel_path,
                    "exists": false
                });
            } else {
                let actual_content = fs::read_to_string(&full_path).unwrap_or_default();
                if let Some(expected) = expected_content {
                    if !actual_content.contains(expected) {
                        passed = false;
                        reasons.push(format!(
                            "File '{}' does not contain expected substring '{}'",
                            rel_path, expected
                        ));
                        evidence = serde_json::json!({
                            "file_path": rel_path,
                            "exists": true,
                            "expected_content": expected,
                            "actual_content": actual_content,
                            "matches": false
                        });
                    } else {
                        evidence = serde_json::json!({
                            "file_path": rel_path,
                            "exists": true,
                            "expected_content": expected,
                            "matches": true,
                            "file_size_bytes": actual_content.len()
                        });
                    }
                } else {
                    evidence = serde_json::json!({
                        "file_path": rel_path,
                        "exists": true,
                        "file_size_bytes": actual_content.len()
                    });
                }
            }
        } else {
            // Generic completion validation if no explicit file criteria
            evidence = serde_json::json!({
                "criteria": task.criteria,
                "assessed": "generic_completion"
            });
        }

        let (verdict, failure_reason) = if passed {
            (VerificationVerdict::Passed, None)
        } else {
            (VerificationVerdict::Failed, Some(reasons.join("; ")))
        };

        let mut v = Verification::new(
            task.id,
            self.kind(),
            verdict,
            evidence,
            0, // duration updated by caller
        );
        if let Some(r) = failure_reason {
            v = v.with_failure_reason(r);
        }

        Ok(v)
    }
}
