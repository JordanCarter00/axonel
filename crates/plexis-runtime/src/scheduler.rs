//! Deterministic Scheduler for Plexis.
//!
//! Orchestrates runnable tasks, agent assignment, lease acquisition, idempotent
//! command queuing, and execution dispatch without provider-specific logic.

use chrono::Duration;
use std::sync::Arc;

use plexis_core::state::{AgentState, TaskState, WorkflowState};
use plexis_core::{Command, CommandTarget, CommandType};
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, ExecutionStore, LeaseStore, SessionStore, TaskStore,
    VerificationStore, WorkflowStore,
};

use crate::dispatcher::CommandDispatcher;
use crate::error::RuntimeError;
use crate::lease_manager::LeaseManager;
use crate::runner::AgentRunner;

/// Deterministic scheduler matching runnable tasks from DAGs to idle agents.
pub struct DeterministicScheduler<
    S: WorkflowStore
        + TaskStore
        + AgentStore
        + SessionStore
        + ExecutionStore
        + CommandStore
        + LeaseStore
        + EventStore
        + VerificationStore
        + 'static,
> {
    store: Arc<S>,
    lease_manager: Arc<LeaseManager>,
    dispatcher: Arc<dyn CommandDispatcher>,
    runner: Arc<AgentRunner<S>>,
}

impl<
        S: WorkflowStore
            + TaskStore
            + AgentStore
            + SessionStore
            + ExecutionStore
            + CommandStore
            + LeaseStore
            + EventStore
            + VerificationStore
            + 'static,
    > DeterministicScheduler<S>
{
    pub fn new(
        store: Arc<S>,
        lease_manager: Arc<LeaseManager>,
        dispatcher: Arc<dyn CommandDispatcher>,
        runner: Arc<AgentRunner<S>>,
    ) -> Self {
        Self {
            store,
            lease_manager,
            dispatcher,
            runner,
        }
    }

    /// Performs one scheduling cycle: discovers runnable tasks, leases them, enqueues commands, and executes.
    pub async fn tick(&self) -> Result<usize, RuntimeError> {
        let mut dispatched_count = 0;

        let workflows = self
            .store
            .list_workflows()
            .await
            .map_err(RuntimeError::Storage)?;

        for wf in workflows {
            if wf.state != WorkflowState::Active {
                continue;
            }

            let graph = self
                .store
                .load_task_graph(&wf.id)
                .await
                .map_err(RuntimeError::Storage)?;

            let runnable = graph.find_runnable_tasks();
            if runnable.is_empty() {
                continue;
            }

            let agents = self
                .store
                .list_agents()
                .await
                .map_err(RuntimeError::Storage)?;

            let mut available_agents: Vec<_> = agents
                .into_iter()
                .filter(|a| a.state == AgentState::Idle)
                .collect();

            for task_id in runnable {
                let mut task = match self
                    .store
                    .get_task(&task_id)
                    .await
                    .map_err(RuntimeError::Storage)?
                {
                    Some(t) => t,
                    None => continue,
                };

                // Only schedule tasks ready for execution
                if task.state != TaskState::Ready && task.state != TaskState::Backlog {
                    continue;
                }

                if available_agents.is_empty() {
                    break;
                }

                let agent = available_agents.remove(0);

                // 1. Acquire exclusive lease
                let lease = match self
                    .lease_manager
                    .acquire(task.id, agent.id, Duration::minutes(5))
                    .await
                {
                    Ok(l) => l,
                    Err(_) => continue, // Task already leased concurrently
                };

                // 2. Transition task to Assigned
                let _ = task.transition_to(TaskState::Ready);
                if task.transition_to(TaskState::Assigned).is_err() {
                    let _ = self.lease_manager.release(&lease.id).await;
                    continue;
                }
                task.assigned_agent_id = Some(agent.id);
                self.store
                    .update_task(&task)
                    .await
                    .map_err(RuntimeError::Storage)?;

                // 3. Enqueue idempotent command
                let idempotency_key =
                    format!("task-exec-{}-attempt-{}", task.id, task.attempts + 1);
                let command = Command::new(
                    CommandTarget::Agent(agent.id),
                    CommandType::ExecuteTask,
                    serde_json::json!({
                        "task_id": task.id.to_string(),
                        "agent_id": agent.id.to_string(),
                        "workflow_id": wf.id.to_string(),
                    }),
                    idempotency_key,
                );

                self.store
                    .enqueue_command(&command)
                    .await
                    .map_err(RuntimeError::Storage)?;

                // 4. Claim and dispatch
                let claimed = self
                    .store
                    .claim_next_queued_command()
                    .await
                    .map_err(RuntimeError::Storage)?
                    .unwrap_or(command);

                self.dispatcher.dispatch(&claimed).await?;

                // 5. Execute via runner
                let exec_res = self.runner.execute_command(&claimed).await;

                // 6. Release lease after attempt
                let _ = self.lease_manager.release(&lease.id).await;

                if exec_res.is_ok() {
                    dispatched_count += 1;
                }
            }
        }

        Ok(dispatched_count)
    }
}
