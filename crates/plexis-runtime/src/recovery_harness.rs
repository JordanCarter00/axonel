//! Systematic failure recovery and crash resumption harness.
//!
//! Provides controlled failure injection, recovery verification, and post-crash
//! reconciliation utilities for autonomous workflow integration tests.

use std::sync::Arc;

use plexis_core::ids::WorkflowId;
use plexis_core::state::TaskState;
use plexis_core::{RecoveryRecord, Task};
use plexis_storage::traits::{EventStore, LeaseStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;

use crate::error::RuntimeError;
use crate::reconciler::{Reconciler, ReconciliationReport};
use crate::recovery::{RecoveryAction, RecoveryController};

/// Harness for deliberate failure injection and crash recovery verification.
pub struct CrashResumptionHarness;

impl CrashResumptionHarness {
    /// Injects a controlled failure into a task's metadata, simulating an incomplete or faulty attempt.
    pub fn inject_failure_metadata(task: &mut Task, failure_reason: &str) {
        task.metadata["injected_failure"] = serde_json::json!({
            "reason": failure_reason,
            "active": true,
        });
    }

    /// Triggers systematic diagnosis, strategy mutation, and state recovery on a task attempt.
    pub async fn diagnose_and_mutate_strategy(
        store: Arc<SqliteStore>,
        task: &mut Task,
        workflow_id: &WorkflowId,
        failure_reason: &str,
    ) -> Result<(RecoveryAction, RecoveryRecord), RuntimeError> {
        let controller = RecoveryController::new(store.clone(), 3);
        let (action, record) = controller
            .diagnose_and_recover(task, workflow_id, None, task.attempts, failure_reason)
            .await?;

        if let RecoveryAction::MutateStrategy {
            ref strategy,
            version,
            ref adjustment,
        } = action
        {
            task.metadata["recovery_advice"] = serde_json::json!({
                "strategy": strategy,
                "version": version,
                "failure": failure_reason,
                "adjustment": adjustment,
            });
            task.assigned_agent_id = None;
            let _ = task.transition_to(TaskState::Ready);
            store
                .update_task(task)
                .await
                .map_err(RuntimeError::Storage)?;
        }

        Ok((action, record))
    }

    /// Reconciles an interrupted SQLite database after process termination/crash.
    pub async fn reconcile_after_crash(
        db_path: &str,
    ) -> Result<(Arc<SqliteStore>, ReconciliationReport), RuntimeError> {
        let store = Arc::new(SqliteStore::open(db_path).map_err(RuntimeError::Storage)?);

        let reconciler = Reconciler::new(
            store.clone() as Arc<dyn TaskStore>,
            store.clone() as Arc<dyn LeaseStore>,
            store.clone() as Arc<dyn EventStore>,
        )
        .with_workflow_store(store.clone() as Arc<dyn WorkflowStore>);

        let report = reconciler.reconcile_startup().await?;
        Ok((store, report))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plexis_core::ids::TaskId;
    use plexis_core::state::WorkflowState;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_failure_injection_and_reconciliation() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("crash_test.db");
        let db_str = db_path.to_str().unwrap();

        let wf_id = WorkflowId::new();
        let t_id = TaskId::new();

        // Phase 1: Setup and crash
        {
            let store = Arc::new(SqliteStore::open(db_str).unwrap());
            let mut wf = plexis_core::Workflow::new("Test WF", "Resilience test");
            wf.id = wf_id;
            wf.state = WorkflowState::Active;
            store.create_workflow(&wf).await.unwrap();

            let mut task = Task::new(wf_id, "Failing compilation task");
            task.id = t_id;
            task.state = TaskState::Running;
            store.create_task(&task).await.unwrap();
            drop(store);
        }

        // Phase 2: Post-crash reconciliation
        let (store2, report) = CrashResumptionHarness::reconcile_after_crash(db_str)
            .await
            .unwrap();
        assert!(report.tasks_unassigned.contains(&t_id));
        assert!(report.resumable_workflows.contains(&wf_id));

        let recovered = store2.get_task(&t_id).await.unwrap().unwrap();
        assert_eq!(recovered.state, TaskState::Ready);
    }
}
