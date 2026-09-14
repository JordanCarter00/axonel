//! Reconciliation and workflow resumption subsystem for Plexis.
//!
//! Reconciles durable expected state against live reality on startup and periodically:
//! - Reclaiming expired leases
//! - Returning abandoned/orphaned tasks from died worker processes back to the ready queue
//! - Identifying runnable workflows for safe resumption
//! - Emitting structured audit telemetry for state repairs

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use plexis_core::ids::{TaskId, WorkflowId};
use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::Event;
use plexis_storage::traits::{EventStore, LeaseStore, TaskStore, WorkflowStore};

use crate::error::RuntimeError;

/// Report summarizing findings and remediation actions taken during reconciliation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconciliationReport {
    /// Tasks whose expired leases were reclaimed.
    pub expired_leases_reclaimed: Vec<TaskId>,
    /// Tasks reset back to Ready state.
    pub tasks_unassigned: Vec<TaskId>,
    /// Active workflows identified as resumable across restarts.
    pub resumable_workflows: Vec<WorkflowId>,
    /// Anomalies detected that could not be automatically resolved.
    pub anomalies: Vec<String>,
}

/// Primary reconciler comparing durable state with runtime reality.
pub struct Reconciler {
    task_store: Arc<dyn TaskStore>,
    lease_store: Arc<dyn LeaseStore>,
    event_store: Arc<dyn EventStore>,
    workflow_store: Option<Arc<dyn WorkflowStore>>,
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
            workflow_store: None,
        }
    }

    pub fn with_workflow_store(mut self, workflow_store: Arc<dyn WorkflowStore>) -> Self {
        self.workflow_store = Some(workflow_store);
        self
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

    /// Performs deep startup reconciliation:
    /// - Reclaims expired leases
    /// - Detects orphaned tasks stuck in Running/Assigned without active leases and resets them to Ready
    /// - Discovers runnable workflows needing resumption
    pub async fn reconcile_startup(&self) -> Result<ReconciliationReport, RuntimeError> {
        let mut report = self.reconcile().await?;

        if let Some(ref wf_store) = self.workflow_store {
            let all_workflows = wf_store
                .list_workflows()
                .await
                .map_err(RuntimeError::Storage)?;
            let active_workflows: Vec<_> = all_workflows
                .into_iter()
                .filter(|w| w.state == WorkflowState::Active)
                .collect();

            for wf in active_workflows {
                let tasks = self
                    .task_store
                    .list_tasks_by_workflow(&wf.id)
                    .await
                    .map_err(RuntimeError::Storage)?;

                let mut has_runnable = false;

                for mut task in tasks {
                    // Check for orphaned execution: running/assigned without lease
                    if task.state == TaskState::Running || task.state == TaskState::Assigned {
                        let active_lease = self.lease_store.get_lease_by_task(&task.id).await?;
                        if active_lease.is_none() {
                            warn!(
                                task_id = %task.id,
                                state = ?task.state,
                                "Startup reconciliation found orphaned task without lease; resetting to Ready"
                            );
                            if let Ok(()) = task.unassign() {
                                self.task_store.update_task(&task).await?;
                                report.tasks_unassigned.push(task.id);

                                let evt = Event::new(
                                    "task",
                                    task.id.to_string(),
                                    "task.startup_orphaned_recovered",
                                    serde_json::json!({
                                        "workflow_id": wf.id.to_string(),
                                        "previous_state": "orphaned",
                                        "new_state": "ready"
                                    }),
                                );
                                let _ = self.event_store.append_event(&evt).await;
                            }
                        }
                    }

                    if task.state.is_runnable_candidate() {
                        has_runnable = true;
                    }
                }

                if has_runnable && wf.state == WorkflowState::Active {
                    report.resumable_workflows.push(wf.id);
                }
            }
        }

        info!(
            reclaimed = report.expired_leases_reclaimed.len(),
            unassigned = report.tasks_unassigned.len(),
            resumable_workflows = report.resumable_workflows.len(),
            "Startup reconciliation finished"
        );

        Ok(report)
    }
}
