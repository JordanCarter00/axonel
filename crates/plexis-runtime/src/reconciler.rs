//! Reconciliation subsystem for Plexis.
//!
//! Reconciles durable expected state against live reality, reclaiming expired
//! leases, returning abandoned tasks to the ready queue, and surfacing anomalies.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use plexis_core::ids::TaskId;
use plexis_core::state::TaskState;
use plexis_core::Event;
use plexis_storage::traits::{EventStore, LeaseStore, TaskStore};

use crate::error::RuntimeError;

/// Report summarizing findings and remediation actions taken during reconciliation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconciliationReport {
    /// Tasks whose expired leases were reclaimed.
    pub expired_leases_reclaimed: Vec<TaskId>,
    /// Tasks reset back to Ready state.
    pub tasks_unassigned: Vec<TaskId>,
    /// Anomalies detected that could not be automatically resolved.
    pub anomalies: Vec<String>,
}

/// Primary reconciler comparing durable state with runtime reality.
pub struct Reconciler {
    task_store: Arc<dyn TaskStore>,
    lease_store: Arc<dyn LeaseStore>,
    event_store: Arc<dyn EventStore>,
}

impl Reconciler {
    pub fn new(
        task_store: Arc<dyn TaskStore>,
        lease_store: Arc<dyn LeaseStore>,
        event_store: Arc<dyn EventStore>,
    ) -> Self {
        Self {
            task_store,
            lease_store,
            event_store,
        }
    }

    /// Reconciles leases and unassigns tasks whose leases have expired.
    pub async fn reconcile(&self) -> Result<ReconciliationReport, RuntimeError> {
        let mut report = ReconciliationReport::default();

        // 1. Reclaim expired leases from storage
        let expired_tasks = self.lease_store.reclaim_expired_leases().await?;

        for task_id in expired_tasks {
            report.expired_leases_reclaimed.push(task_id);

            // 2. Fetch task and inspect its state
            if let Some(mut task) = self.task_store.get_task(&task_id).await? {
                if task.state == TaskState::Assigned || task.state == TaskState::Running {
                    warn!(
                        "Reconciliation: Task '{}' lease expired while in {:?}. Unassigning to Ready.",
                        task_id, task.state
                    );

                    // Unassign task safely returning to Ready
                    if let Err(e) = task.unassign() {
                        report.anomalies.push(format!(
                            "Failed to transition task '{task_id}' to Ready: {e}"
                        ));
                        continue;
                    }

                    self.task_store.update_task(&task).await?;
                    report.tasks_unassigned.push(task_id);

                    // Record audit event
                    let evt = Event::new(
                        "task",
                        task_id.to_string(),
                        "task.lease_reclaimed",
                        serde_json::json!({
                            "reason": "lease_expired",
                            "new_state": "ready"
                        }),
                    );
                    let _ = self.event_store.append_event(&evt).await;
                }
            } else {
                report
                    .anomalies
                    .push(format!("Lease existed for non-existent task '{task_id}'"));
            }
        }

        if !report.expired_leases_reclaimed.is_empty() {
            info!(
                "Reconciliation complete: reclaimed {} leases, reset {} tasks to Ready",
                report.expired_leases_reclaimed.len(),
                report.tasks_unassigned.len()
            );
        }

        Ok(report)
    }
}
