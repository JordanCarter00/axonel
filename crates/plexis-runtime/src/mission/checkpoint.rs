//! Mission checkpoint creation and restoration.

use std::sync::Arc;

use plexis_core::ids::{ExecutionId, MissionId, WorkflowId};
use plexis_core::mission::{MissionBudgetConsumed, MissionCheckpoint};
use plexis_core::state::TaskState;
use plexis_storage::traits::{MissionStore, TaskStore};

use crate::error::RuntimeError;

pub struct CheckpointManager<S> {
    store: Arc<S>,
}

impl<S: MissionStore + TaskStore + 'static> CheckpointManager<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// Creates and persists a durable mission checkpoint in SQLite.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_checkpoint(
        &self,
        mission_id: MissionId,
        cycle_index: u32,
        workflow_id: WorkflowId,
        budget_consumed: MissionBudgetConsumed,
        latest_verified_commit: Option<String>,
        active_executions: Vec<ExecutionId>,
        planner_context: serde_json::Value,
    ) -> Result<MissionCheckpoint, RuntimeError> {
        let tasks = self.store.list_tasks_by_workflow(&workflow_id).await?;

        let mut task_states_summary = serde_json::Map::new();
        let mut completed_tasks = Vec::new();
        let mut unresolved_tasks = Vec::new();

        for t in tasks {
            task_states_summary.insert(t.id.to_string(), serde_json::json!(t.state.as_str()));
            if t.state == TaskState::Verified {
                completed_tasks.push(t.id);
            } else if !t.state.is_terminal() {
                unresolved_tasks.push(t.id);
            }
        }

        let mut ckpt = MissionCheckpoint::new(
            mission_id,
            cycle_index,
            workflow_id,
            serde_json::Value::Object(task_states_summary),
            budget_consumed,
        );
        ckpt.active_executions = active_executions;
        ckpt.completed_tasks = completed_tasks;
        ckpt.unresolved_tasks = unresolved_tasks;
        ckpt.latest_verified_commit = latest_verified_commit;
        ckpt.planner_context = planner_context;

        self.store.create_checkpoint(&ckpt).await?;

        Ok(ckpt)
    }

    /// Restores the latest durable checkpoint for a mission.
    pub async fn get_latest_checkpoint(
        &self,
        mission_id: &MissionId,
    ) -> Result<Option<MissionCheckpoint>, RuntimeError> {
        Ok(self.store.get_latest_checkpoint(mission_id).await?)
    }
}
