use chrono::Duration;
use std::sync::Arc;

use plexis_core::state::{AgentState, TaskState, WorkflowState};
use plexis_core::{Command, CommandTarget, CommandType};
use plexis_storage::traits::{
    AgentStore, ApprovalStore, CommandStore, EventStore, ExecutionStore, LeaseStore, MemoryStore,
    MessageStore, RecoveryStore, SessionStore, TaskStore, VerificationStore, WorkflowStore,
};

use crate::dispatcher::CommandDispatcher;
use crate::error::RuntimeError;
use crate::lease_manager::LeaseManager;
use crate::runner::AgentRunner;
use crate::selector::AgentSelector;

/// Deterministic scheduler matching runnable tasks from DAGs to capability-matched agents.
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
        + MessageStore
        + ApprovalStore
        + MemoryStore
        + RecoveryStore
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
            + MessageStore
            + ApprovalStore
            + MemoryStore
            + RecoveryStore
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
    ///
    /// Independent tasks execute concurrently in parallel across matched agents.
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
                .filter(|a| a.state == AgentState::Idle && a.current_execution_id.is_none())
                .collect();

            let mut execution_jobs = Vec::new();

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

                // Select agent based on declared capabilities and task requirements
                let selected_agent =
                    match AgentSelector::select_best_agent(&task, &available_agents) {
                        Some(agent) => agent.clone(),
                        None => continue, // No idle agent satisfies this task's required capabilities
                    };

                // 1. Acquire exclusive lease under monotonic fencing token
                let lease = match self
                    .lease_manager
                    .acquire(task.id, selected_agent.id, Duration::minutes(5))
                    .await
                {
                    Ok(l) => l,
                    Err(_) => continue, // Task already leased concurrently
                };

                // Remove selected agent from candidate pool for this tick
                available_agents.retain(|a| a.id != selected_agent.id);

                // 2. Transition task to Assigned
                let _ = task.transition_to(TaskState::Ready);
                if task.transition_to(TaskState::Assigned).is_err() {
                    let _ = self.lease_manager.release(&lease.id).await;
                    continue;
                }
                task.assigned_agent_id = Some(selected_agent.id);
                self.store
                    .update_task(&task)
                    .await
                    .map_err(RuntimeError::Storage)?;

                // 3. Enqueue idempotent command
                let idempotency_key =
                    format!("task-exec-{}-attempt-{}", task.id, task.attempts + 1);
                let command = Command::new(
                    CommandTarget::Agent(selected_agent.id),
                    CommandType::ExecuteTask,
                    serde_json::json!({
                        "task_id": task.id.to_string(),
                        "agent_id": selected_agent.id.to_string(),
                        "workflow_id": wf.id.to_string(),
                        "lease_id": lease.id.to_string(),
                        "lease_generation": lease.generation,
                    }),
                    idempotency_key,
                );

                self.store
                    .enqueue_command(&command)
                    .await
                    .map_err(RuntimeError::Storage)?;

                // 4. Claim next queued command
                let claimed = self
                    .store
                    .claim_next_queued_command()
                    .await
                    .map_err(RuntimeError::Storage)?
                    .unwrap_or(command);

                execution_jobs.push((claimed, lease.id));
            }

            // 5. Execute all independent matched tasks concurrently in parallel!
            if !execution_jobs.is_empty() {
                let mut handles = Vec::new();
                for (claimed_cmd, lease_id) in execution_jobs {
                    let dispatcher = self.dispatcher.clone();
                    let runner = self.runner.clone();
                    let lease_mgr = self.lease_manager.clone();

                    handles.push(async move {
                        let _ = dispatcher.dispatch(&claimed_cmd).await;
                        let exec_res = runner.execute_command(&claimed_cmd).await;
                        let _ = lease_mgr.release(&lease_id).await;
                        exec_res.is_ok()
                    });
                }

                let results = futures::future::join_all(handles).await;
                dispatched_count += results.into_iter().filter(|&ok| ok).count();
            }
        }

        Ok(dispatched_count)
    }
}
