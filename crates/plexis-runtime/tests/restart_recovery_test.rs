use std::sync::Arc;
use tempfile::tempdir;

use plexis_core::ids::{AgentId, TaskId, WorkflowId};
use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::{Agent, ExecutionProfile, Task, Workflow};
use plexis_runtime::reconciler::Reconciler;
use plexis_storage::traits::{AgentStore, EventStore, LeaseStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_crash_and_startup_reconciliation_resumption() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("plexis_resilience.db");
    let db_str = db_path.to_str().unwrap();

    let workflow_id = WorkflowId::new();
    let agent_id = AgentId::new();
    let task1_id = TaskId::new();
    let task2_id = TaskId::new();

    // PHASE 1: Runtime Instance 1 runs and suddenly "crashes"
    {
        let store = Arc::new(SqliteStore::open(db_str).expect("open db"));

        // 1. Setup workflow and agent
        let mut workflow = Workflow::new(
            "Build Resilient Agent Platform",
            "Build resilient autonomous runtime",
        );
        workflow.id = workflow_id;
        workflow.state = WorkflowState::Active;
        store.create_workflow(&workflow).await.expect("create wf");

        let mut agent = Agent::new(
            "Worker",
            "general_worker",
            ExecutionProfile::new("openai", "gpt-4o"),
        );
        agent.id = agent_id;
        store.create_agent(&agent).await.expect("create agent");

        // Task 1 was already verified/completed
        let mut task1 = Task::new(workflow_id, "Setup build environment");
        task1.id = task1_id;
        task1.state = TaskState::Verified;
        store.create_task(&task1).await.expect("create task1");

        // Task 2 was actively RUNNING when the process crashed (no active lease renewed)
        let mut task2 = Task::new(workflow_id, "Compile binary and run test suite");
        task2.id = task2_id;
        task2.state = TaskState::Running;
        task2.assigned_agent_id = Some(agent_id);
        store.create_task(&task2).await.expect("create task2");

        // CRASH: The process terminates here without cleaning up task2 or unassigning it.
        drop(store);
    }

    // PHASE 2: New Runtime Instance 2 starts up on the existing persistent database
    {
        let store2 = Arc::new(SqliteStore::open(db_str).expect("reopen db after crash"));

        let reconciler = Reconciler::new(
            store2.clone() as Arc<dyn TaskStore>,
            store2.clone() as Arc<dyn LeaseStore>,
            store2.clone() as Arc<dyn EventStore>,
        )
        .with_workflow_store(store2.clone() as Arc<dyn WorkflowStore>);

        // Run deep startup reconciliation
        let report = reconciler
            .reconcile_startup()
            .await
            .expect("startup reconciliation");

        // Verify task2 was recognized as orphaned and reset to Ready
        assert!(
            report.tasks_unassigned.contains(&task2_id),
            "Orphaned running task must be unassigned back to Ready on startup"
        );

        // Verify the active workflow was flagged as resumable
        assert!(
            report.resumable_workflows.contains(&workflow_id),
            "Workflow with recovered task must be marked as resumable"
        );

        // Inspect durable state of task2 in storage
        let recovered_task = store2
            .get_task(&task2_id)
            .await
            .expect("get task")
            .expect("task exists");
        assert_eq!(recovered_task.state, TaskState::Ready);
        assert!(recovered_task.assigned_agent_id.is_none());

        // Verify audit event was persisted
        let events = store2.list_recent_events(10).await.expect("list events");
        assert!(events
            .iter()
            .any(|e| e.event_type == "task.startup_orphaned_recovered"));
    }
}
