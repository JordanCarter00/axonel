use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use plexis_core::state::TaskState;
use plexis_core::{Agent, ExecutionProfile, Task, Workflow};
use plexis_providers::adapters::ScriptedProvider;
use plexis_providers::types::CompletionResponse;
use plexis_runtime::dispatcher::BroadcastCommandDispatcher;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::scheduler::DeterministicScheduler;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::sqlite::SqliteStore;
use plexis_storage::traits::{AgentStore, TaskStore, WorkflowStore};
use plexis_tools::ToolRegistry;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_high_contention_concurrency_stress() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite in-memory"));

    // 1. Create Workflow
    let mut wf = Workflow::new(
        "High-Contention Concurrency Workflow",
        "Process 25 concurrent tasks across 5 agents with overlapping scheduler ticks",
    );
    wf.set_state(plexis_core::state::WorkflowState::Active);
    store.create_workflow(&wf).await.unwrap();

    // 2. Register 5 Agents
    let mut agent_ids = Vec::new();
    for i in 1..=5 {
        let agent = Agent::new(
            format!("Agent-{}", i),
            "Worker",
            ExecutionProfile::new(format!("mock-provider-{}", i), format!("mock-model-{}", i)),
        );
        store.create_agent(&agent).await.unwrap();
        agent_ids.push(agent.id);
    }

    // 3. Create 25 Independent Tasks
    let task_count = 25;
    let mut task_ids = Vec::new();
    for i in 1..=task_count {
        let mut task = Task::new(wf.id, format!("Task #{}", i));
        task.state = TaskState::Ready;
        store.create_task(&task).await.unwrap();
        task_ids.push(task.id);
    }

    // 4. Setup Runtime Components
    let lease_mgr = Arc::new(LeaseManager::new(store.clone()));
    let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));

    let verifier = Arc::new(WorkspaceVerifier::new(store.clone()));
    let tool_reg = ToolRegistry::new();
    let mut runner = AgentRunner::new(store.clone(), tool_reg, verifier);

    // Register a mock provider for each agent model
    for i in 1..=5 {
        let provider = Arc::new(ScriptedProvider::new(format!("mock-provider-{}", i)));
        // Pre-fill responses so any task execution completes cleanly
        for _ in 0..10 {
            provider.queue_response(CompletionResponse::text(format!(
                "Completed by agent {}",
                i
            )));
        }
        runner.register_provider(provider);
    }
    let runner = Arc::new(runner);

    let scheduler = Arc::new(DeterministicScheduler::new(
        store.clone(),
        lease_mgr,
        dispatcher,
        runner,
    ));

    // 5. Spawn 5 Concurrent Overlapping Schedulers / Workers
    let mut handles: Vec<JoinHandle<usize>> = Vec::new();
    for _ in 0..5 {
        let sched = scheduler.clone();
        handles.push(tokio::spawn(async move {
            let mut total_executed = 0;
            for _ in 0..15 {
                if let Ok(count) = sched.tick().await {
                    total_executed += count;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            total_executed
        }));
    }

    // Wait for all concurrent tick loops to complete
    let mut total_dispatched = 0;
    for h in handles {
        total_dispatched += h.await.unwrap();
    }

    // 6. Assertions
    // Re-query all tasks from storage
    let tasks = store.list_tasks_by_workflow(&wf.id).await.unwrap();
    assert_eq!(tasks.len(), task_count);

    // Verify all 25 tasks were claimed and processed
    assert!(
        total_dispatched >= task_count,
        "Total dispatched jobs across ticks must cover all tasks (got {})",
        total_dispatched
    );

    // Verify zero tasks in Backlog and every task has been attempted at least once
    for t in &tasks {
        assert_ne!(
            t.state,
            TaskState::Backlog,
            "No task should be stuck in Backlog"
        );
        assert!(
            t.attempts >= 1,
            "Every task must have been attempted at least once"
        );
    }
}
