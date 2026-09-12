use std::sync::Arc;

use plexis_core::ids::{ExecutionId, TaskId, WorkflowId};
use plexis_core::{RecoveryResult, Task, Workflow};
use plexis_runtime::recovery::{RecoveryAction, RecoveryController};
use plexis_storage::traits::{RecoveryStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_recovery_controller_strategy_mutation_and_loop_detection() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));
    let controller = RecoveryController::new(store.clone(), 4);

    let workflow_id = WorkflowId::new();
    let mut workflow = Workflow::new("Recovery Workflow", "Testing recovery controller");
    workflow.id = workflow_id;
    store.create_workflow(&workflow).await.expect("create wf");

    let task_id = TaskId::new();
    let mut task = Task::new(workflow_id, "Execute integration suite");
    task.id = task_id;
    store.create_task(&task).await.expect("create task");

    let exec_id = ExecutionId::new();

    // 1. First failure: tool execution failure (e.g. exit code 1)
    let (action1, record1) = controller
        .diagnose_and_recover(
            &task,
            &workflow_id,
            Some(&exec_id),
            1,
            "Command failed with exit code 1: missing dependency libssl",
        )
        .await
        .expect("recover attempt 1");

    match action1 {
        RecoveryAction::MutateStrategy {
            strategy, version, ..
        } => {
            assert_eq!(strategy, "tool_adaptation");
            assert_eq!(version, 1);
        }
        other => panic!("Expected MutateStrategy, got {:?}", other),
    }
    assert_eq!(record1.strategy_version, 1);
    assert_eq!(record1.result, RecoveryResult::InProgress);

    // 2. Second failure: same error happens again
    let (action2, _record2) = controller
        .diagnose_and_recover(
            &task,
            &workflow_id,
            Some(&exec_id),
            2,
            "Command failed with exit code 1: missing dependency libssl",
        )
        .await
        .expect("recover attempt 2");

    match action2 {
        RecoveryAction::MutateStrategy {
            strategy, version, ..
        } => {
            assert_eq!(strategy, "tool_adaptation");
            assert_eq!(version, 2, "Strategy version should increment on mutation");
        }
        other => panic!("Expected MutateStrategy, got {:?}", other),
    }

    // 3. Third failure: same error happens a third time -> LOOP DETECTED -> REPLAN
    let (action3, record3) = controller
        .diagnose_and_recover(
            &task,
            &workflow_id,
            Some(&exec_id),
            3,
            "Command failed with exit code 1: missing dependency libssl",
        )
        .await
        .expect("recover attempt 3");

    match action3 {
        RecoveryAction::ReplanWorkflow { reason } => {
            assert!(
                reason.contains("Repeated failure pattern"),
                "Should detect loop and trigger replanning"
            );
        }
        other => panic!("Expected ReplanWorkflow, got {:?}", other),
    }
    assert_eq!(record3.result, RecoveryResult::Escalated);

    // 4. Verify historical recovery records in storage
    let history = store
        .list_recovery_records_by_task(&task_id)
        .await
        .expect("list recovery records");
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].attempt, 1);
    assert_eq!(history[1].attempt, 2);
    assert_eq!(history[2].attempt, 3);
}

#[tokio::test]
async fn test_recovery_controller_max_attempts_fatal_escalation() {
    let store = Arc::new(SqliteStore::open_in_memory().expect("open sqlite"));
    let controller = RecoveryController::new(store.clone(), 3);

    let workflow_id = WorkflowId::new();
    let mut workflow = Workflow::new("Recovery Workflow 2", "Testing max recovery attempts");
    workflow.id = workflow_id;
    store.create_workflow(&workflow).await.expect("create wf");

    let task = Task::new(workflow_id, "Parse ambiguous spec");
    store.create_task(&task).await.expect("create task");

    // Attempt 3 on a max of 3 attempts triggers EscalateFatal
    let (action, record) = controller
        .diagnose_and_recover(
            &task,
            &workflow_id,
            None,
            3,
            "Unresolvable parser ambiguity",
        )
        .await
        .expect("recover attempt 3");

    match action {
        RecoveryAction::EscalateFatal { reason } => {
            assert!(reason.contains("exceeded maximum recovery attempts"));
        }
        other => panic!("Expected EscalateFatal, got {:?}", other),
    }
    assert_eq!(record.result, RecoveryResult::Escalated);
}
