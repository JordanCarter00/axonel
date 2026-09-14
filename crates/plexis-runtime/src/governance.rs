//! Durable Human Governance and Approval Gate manager.
//!
//! Orchestrates approval requests, pauses tasks in `NeedsHuman` state,
//! processes human authorization decisions, and resumes tasks safely.

use std::sync::Arc;

use plexis_core::ids::{AgentId, ApprovalId, TaskId, WorkflowId};
use plexis_core::state::TaskState;
use plexis_core::{ApprovalRecord, ApprovalState, Event, Task};
use plexis_storage::traits::{ApprovalStore, EventStore, TaskStore};

use crate::error::RuntimeError;

/// Manages durable approval gates and human-in-the-loop decisions.
pub struct GovernanceManager<S: TaskStore + ApprovalStore + EventStore + 'static> {
    store: Arc<S>,
}

impl<S: TaskStore + ApprovalStore + EventStore + 'static> GovernanceManager<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// Requests explicit human approval for a sensitive action or policy check.
    ///
    /// Transitions the task to `TaskState::NeedsHuman` and creates a pending approval record.
    pub async fn request_approval(
        &self,
        task_id: TaskId,
        workflow_id: WorkflowId,
        requester: Option<AgentId>,
        action_description: impl Into<String>,
        reason: Option<String>,
    ) -> Result<ApprovalRecord, RuntimeError> {
        let mut approval = ApprovalRecord::new(task_id, workflow_id, action_description);
        if let Some(agent_id) = requester {
            approval = approval.with_requester(agent_id);
        }
        if let Some(r) = reason {
            approval = approval.with_reason(r);
        }

        // 1. Save approval record
        self.store
            .create_approval(&approval)
            .await
            .map_err(RuntimeError::Storage)?;

        // 2. Transition task to NeedsHuman
        if let Some(mut task) = self
            .store
            .get_task(&task_id)
            .await
            .map_err(RuntimeError::Storage)?
        {
            let _ = task.transition_to(TaskState::NeedsHuman);
            self.store
                .update_task(&task)
                .await
                .map_err(RuntimeError::Storage)?;
        }

        // 3. Emit approval_requested event
        let evt = Event::new(
            "approval",
            approval.id.to_string(),
            "approval_requested",
            serde_json::json!({
                "task_id": task_id.to_string(),
                "workflow_id": workflow_id.to_string(),
                "description": approval.action_description,
            }),
        );
        let _ = self.store.append_event(&evt).await;

        Ok(approval)
    }

    /// Submits a human decision (`true` for approved, `false` for rejected).
    pub async fn submit_decision(
        &self,
        approval_id: &ApprovalId,
        approved: bool,
        notes: Option<String>,
    ) -> Result<ApprovalRecord, RuntimeError> {
        let mut approval = self
            .store
            .get_approval(approval_id)
            .await
            .map_err(RuntimeError::Storage)?
            .ok_or_else(|| {
                RuntimeError::InvalidCommand(format!("Approval '{}' not found", approval_id))
            })?;

        if approval.state != ApprovalState::Pending {
            return Err(RuntimeError::InvalidCommand(format!(
                "Approval '{}' is already in state '{:?}'",
                approval_id, approval.state
            )));
        }

        let task_id = approval.task_id;
        let mut task = self
            .store
            .get_task(&task_id)
            .await
            .map_err(RuntimeError::Storage)?
            .ok_or_else(|| RuntimeError::InvalidCommand(format!("Task '{}' not found", task_id)))?;

        if approved {
            approval
                .approve(notes.clone())
                .map_err(|e| RuntimeError::InvalidCommand(e.to_string()))?;
            self.store
                .update_approval(&approval)
                .await
                .map_err(RuntimeError::Storage)?;

            // Transition task back to Ready for resumption
            let _ = task.transition_to(TaskState::Ready);
            self.store
                .update_task(&task)
                .await
                .map_err(RuntimeError::Storage)?;

            let evt = Event::new(
                "approval",
                approval.id.to_string(),
                "approval_granted",
                serde_json::json!({
                    "task_id": task_id.to_string(),
                    "notes": notes,
                }),
            );
            let _ = self.store.append_event(&evt).await;
        } else {
            approval
                .reject(notes.clone())
                .map_err(|e| RuntimeError::InvalidCommand(e.to_string()))?;
            self.store
                .update_approval(&approval)
                .await
                .map_err(RuntimeError::Storage)?;

            // Transition task to Failed upon rejection
            let _ = task.transition_to(TaskState::Failed);
            self.store
                .update_task(&task)
                .await
                .map_err(RuntimeError::Storage)?;

            let evt = Event::new(
                "approval",
                approval.id.to_string(),
                "approval_rejected",
                serde_json::json!({
                    "task_id": task_id.to_string(),
                    "notes": notes,
                }),
            );
            let _ = self.store.append_event(&evt).await;
        }

        Ok(approval)
    }

    /// Resumes a paused or human-held task.
    pub async fn resume_task(&self, task_id: &TaskId) -> Result<Task, RuntimeError> {
        let mut task = self
            .store
            .get_task(task_id)
            .await
            .map_err(RuntimeError::Storage)?
            .ok_or_else(|| RuntimeError::InvalidCommand(format!("Task '{}' not found", task_id)))?;

        let _ = task.transition_to(TaskState::Ready);
        self.store
            .update_task(&task)
            .await
            .map_err(RuntimeError::Storage)?;

        let evt = Event::new(
            "task",
            task_id.to_string(),
            "task_resumed",
            serde_json::json!({
                "workflow_id": task.workflow_id.to_string(),
            }),
        );
        let _ = self.store.append_event(&evt).await;

        Ok(task)
    }
}
