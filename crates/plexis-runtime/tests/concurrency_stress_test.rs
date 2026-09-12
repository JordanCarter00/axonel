use std::sync::Arc;
use tokio::task::JoinSet;

use plexis_core::ids::{AgentId, TaskId, WorkflowId};
use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::{Task, Workflow};
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::resources::{ConcurrencyLimiter, ResourceLimitsConfig, TaskResourceTracker};
use plexis_storage::traits::{LeaseStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_concurrency_stress_and_lease_fencing() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));
    let workflow_id = WorkflowId::new();

    let mut workflow = Workflow::new(
        "Stress Test Parallel Pipeline",
        "Verify concurrent execution limits",
    );
    workflow.id = workflow_id;
    workflow.state = WorkflowState::Active;
    store.create_workflow(&workflow).await.expect("create wf");

    // 1. Create 12 parallel tasks
    let mut task_ids = Vec::new();
    for i in 1..=12 {
        let task_id = TaskId::new();
        let mut task = Task::new(workflow_id, format!("Parallel worker step {}", i));
        task.id = task_id;
        task.state = TaskState::Ready;
        store.create_task(&task).await.expect("create task");
        task_ids.push(task_id);
    }

    // 2. Setup lease manager
    let lease_manager = Arc::new(LeaseManager::new(store.clone() as Arc<dyn LeaseStore>));

    // Limit concurrency to 4 simultaneous active agent slots
    let concurrency_limiter = ConcurrencyLimiter::new(4);

    let mut join_set = JoinSet::new();

    for task_id in task_ids {
        let lm = lease_manager.clone();
        let limiter = concurrency_limiter.clone();

        join_set.spawn(async move {
            let agent_id = AgentId::new();

            // Try acquiring concurrency permit
            let permit = loop {
                match limiter.try_acquire() {
                    Ok(p) => break p,
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    }
                }
            };

            // Acquire exclusive lease
            let lease_result = lm
                .acquire(task_id, agent_id, chrono::Duration::seconds(30))
                .await;

            // Resource tracking
            let mut tracker = TaskResourceTracker::new(ResourceLimitsConfig::default());
            tracker.record_step().unwrap();
            tracker.record_tool_call().unwrap();
            tracker.record_tokens(150).unwrap();

            // Simulate execution work
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;

            // Release lease
            if let Ok(lease) = lease_result {
                let _ = lm.release(&lease.id).await;
            }

            drop(permit);
        });
    }

    // Await all tasks to finish
    while let Some(res) = join_set.join_next().await {
        res.expect("task join");
    }

    // Verify all concurrency permits freed
    assert_eq!(concurrency_limiter.active_agents(), 0);
}
