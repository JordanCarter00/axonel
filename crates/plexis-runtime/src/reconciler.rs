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

use plexis_core::ids::{CommandId, ExecutionId, MissionId, TaskId, WorkflowId};
use plexis_core::state::{CommandState, MissionState, TaskState, WorkflowState};
use plexis_core::Event;
use plexis_storage::traits::{
    CommandStore, EventStore, ExecutionStore, LeaseStore, MissionStore, TaskStore, WorkflowStore,
};

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
    /// Active missions identified as resumable across restarts.
    pub resumable_missions: Vec<MissionId>,
    /// Orphaned or abandoned commands resolved during reconciliation.
    pub commands_reconciled: Vec<CommandId>,
    /// Orphaned external or in-flight executions resolved during reconciliation.
    pub executions_reconciled: Vec<ExecutionId>,
    /// Anomalies detected that could not be automatically resolved.
    pub anomalies: Vec<String>,
}

/// Primary reconciler comparing durable state with runtime reality.
pub struct Reconciler {
    task_store: Arc<dyn TaskStore>,
    lease_store: Arc<dyn LeaseStore>,
    event_store: Arc<dyn EventStore>,
    workflow_store: Option<Arc<dyn WorkflowStore>>,
    command_store: Option<Arc<dyn CommandStore>>,
    execution_store: Option<Arc<dyn ExecutionStore>>,
    mission_store: Option<Arc<dyn MissionStore>>,
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
            command_store: None,
            execution_store: None,
            mission_store: None,
        }
    }

    pub fn with_workflow_store(mut self, workflow_store: Arc<dyn WorkflowStore>) -> Self {
        self.workflow_store = Some(workflow_store);
        self
    }

    pub fn with_command_store(mut self, command_store: Arc<dyn CommandStore>) -> Self {
        self.command_store = Some(command_store);
        self
    }

    pub fn with_execution_store(mut self, execution_store: Arc<dyn ExecutionStore>) -> Self {
        self.execution_store = Some(execution_store);
        self
    }

    pub fn with_mission_store(mut self, mission_store: Arc<dyn MissionStore>) -> Self {
        self.mission_store = Some(mission_store);
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

                    // Reconcile any in-flight executions for this task
                    if let Some(ref exec_store) = self.execution_store {
                        if let Ok(execs) = exec_store.list_executions_by_task(&task_id).await {
                            for mut exec in execs {
                                if exec.state == plexis_core::state::ExecutionState::Running {
                                    let _ = exec.mark_failed(
                                        "Orphaned external agent execution reconciled after server restart / lease expiry",
                                    );
                                    let _ = exec_store.update_execution(&exec).await;
                                    report.executions_reconciled.push(exec.id);

                                    let exec_evt = Event::new(
                                        "execution",
                                        exec.id.to_string(),
                                        "execution_failed",
                                        serde_json::json!({
                                            "task_id": task_id.to_string(),
                                            "reason": "server_restart_orphaned_reconciled",
                                        }),
                                    );
                                    let _ = self.event_store.append_event(&exec_evt).await;
                                }
                            }
                        }
                    }

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

        // 3. Reconcile orphaned/in-flight commands
        self.reconcile_commands(&mut report).await?;

        Ok(report)
    }

    /// Reconciles orphaned or in-flight commands after restarts or lease expirations.
    pub async fn reconcile_commands(
        &self,
        report: &mut ReconciliationReport,
    ) -> Result<(), RuntimeError> {
        let cmd_store = match &self.command_store {
            Some(cs) => cs,
            None => return Ok(()),
        };

        // Query commands currently marked as Dispatched or Delivered
        let mut in_flight = cmd_store
            .list_commands_by_state(CommandState::Dispatched)
            .await
            .map_err(RuntimeError::Storage)?;

        let delivered = cmd_store
            .list_commands_by_state(CommandState::Delivered)
            .await
            .map_err(RuntimeError::Storage)?;

        in_flight.extend(delivered);

        for mut cmd in in_flight {
            let target_task_id = match &cmd.target {
                plexis_core::CommandTarget::Task(tid) => Some(*tid),
                _ => cmd
                    .payload
                    .get("task_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<TaskId>().ok()),
            };

            if let Some(task_id) = target_task_id {
                let task = self.task_store.get_task(&task_id).await?;
                let active_lease = self.lease_store.get_lease_by_task(&task_id).await?;

                let should_cancel = match task {
                    None => true,
                    Some(t) => {
                        (t.state != TaskState::Running && t.state != TaskState::Assigned)
                            || active_lease.is_none()
                    }
                };

                if should_cancel {
                    warn!(
                        command_id = %cmd.id,
                        task_id = %task_id,
                        old_state = ?cmd.state,
                        "Reconciliation: In-flight command orphaned or task no longer running; marking Failed"
                    );
                    cmd.mark_failed();
                    cmd_store
                        .update_command(&cmd)
                        .await
                        .map_err(RuntimeError::Storage)?;
                    report.commands_reconciled.push(cmd.id);

                    let evt = Event::new(
                        "command",
                        cmd.id.to_string(),
                        "command.orphaned_reconciled",
                        serde_json::json!({
                            "task_id": task_id.to_string(),
                            "previous_state": "in_flight",
                            "new_state": cmd.state.as_str(),
                        }),
                    );
                    let _ = self.event_store.append_event(&evt).await;
                }
            }
        }

        Ok(())
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

                                // Reconcile any in-flight executions for this task
                                if let Some(ref exec_store) = self.execution_store {
                                    if let Ok(execs) =
                                        exec_store.list_executions_by_task(&task.id).await
                                    {
                                        for mut exec in execs {
                                            if exec.state
                                                == plexis_core::state::ExecutionState::Running
                                            {
                                                let _ = exec.mark_failed(
                                                    "Orphaned external agent execution reconciled during startup",
                                                );
                                                let _ = exec_store.update_execution(&exec).await;
                                                report.executions_reconciled.push(exec.id);

                                                let exec_evt = Event::new(
                                                    "execution",
                                                    exec.id.to_string(),
                                                    "execution_failed",
                                                    serde_json::json!({
                                                        "task_id": task.id.to_string(),
                                                        "reason": "startup_orphaned_reconciled",
                                                    }),
                                                );
                                                let _ =
                                                    self.event_store.append_event(&exec_evt).await;
                                            }
                                        }
                                    }
                                }

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

        // Reconcile active missions across server restarts
        if let Some(ref m_store) = self.mission_store {
            if let Ok(missions) = m_store.list_missions().await {
                for m in missions {
                    if matches!(
                        m.state,
                        MissionState::Planning
                            | MissionState::Running
                            | MissionState::Replanning
                            | MissionState::Verifying
                    ) {
                        info!(
                            mission_id = %m.id,
                            state = ?m.state,
                            "Startup reconciliation identified active mission for resumption"
                        );
                        report.resumable_missions.push(m.id);
                        let evt = Event::new(
                            "mission",
                            m.id.to_string(),
                            "mission.reconciled_resumable",
                            serde_json::json!({
                                "state": m.state.as_str(),
                                "cycle_index": m.cycle_index,
                            }),
                        );
                        let _ = self.event_store.append_event(&evt).await;
                    }
                }
            }
        }

        info!(
            reclaimed = report.expired_leases_reclaimed.len(),
            unassigned = report.tasks_unassigned.len(),
            resumable_workflows = report.resumable_workflows.len(),
            resumable_missions = report.resumable_missions.len(),
            "Startup reconciliation finished"
        );

        Ok(report)
    }
}
