use std::sync::Arc;

use plexis_core::ids::WorkflowId;
use plexis_core::state::TaskState;
use plexis_core::{Execution, Task, Verification, VerificationVerdict, Workflow};
use plexis_planner::{
    FailureDiagnoser, PlanApplier, PlanProposal, PlanValidator, PlanningContext, ProposedTask,
    RecoveryAction,
};
use plexis_storage::traits::{PlanStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;

#[test]
fn test_invalid_planner_proposal_empty_tasks() {
    let proposal = PlanProposal::new("Empty", "No tasks");
    let report = PlanValidator::validate(&proposal);
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("at least one task")));
}

#[test]
fn test_planner_generated_cycle_detection() {
    let mut proposal = PlanProposal::new("Cyclic Plan", "Has circular dependency");
    proposal.tasks.push(ProposedTask {
        temp_id: "task-a".into(),
        objective: "Task A".into(),
        description: None,
        criteria: vec!["file:a.txt".into()],
        required_capabilities: vec![],
        suggested_role: None,
        priority: 0,
    });
    proposal.tasks.push(ProposedTask {
        temp_id: "task-b".into(),
        objective: "Task B".into(),
        description: None,
        criteria: vec!["file:b.txt".into()],
        required_capabilities: vec![],
        suggested_role: None,
        priority: 0,
    });

    // task-b depends on task-a, task-a depends on task-b
    proposal = proposal.with_dependency("task-b", "task-a");
    proposal = proposal.with_dependency("task-a", "task-b");

    let report = PlanValidator::validate(&proposal);
    assert!(!report.is_valid);
    assert!(report.errors.iter().any(|e| e.contains("Cycle detected")));
}

#[test]
fn test_duplicate_task_id_rejection() {
    let mut proposal = PlanProposal::new("Duplicate IDs", "Two tasks with same id");
    proposal.tasks.push(ProposedTask {
        temp_id: "task-dup".into(),
        objective: "First".into(),
        criteria: vec!["file:first.txt".into()],
        description: None,
        required_capabilities: vec![],
        suggested_role: None,
        priority: 0,
    });
    proposal.tasks.push(ProposedTask {
        temp_id: "task-dup".into(),
        objective: "Second".into(),
        criteria: vec!["file:second.txt".into()],
        description: None,
        required_capabilities: vec![],
        suggested_role: None,
        priority: 0,
    });

    let report = PlanValidator::validate(&proposal);
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("Duplicate task temporary id")));
}

#[test]
fn test_unverifiable_empty_criteria_rejection() {
    let mut proposal = PlanProposal::new("No criteria", "Task missing criteria");
    proposal.tasks.push(ProposedTask {
        temp_id: "task-1".into(),
        objective: "Objective with no criteria".into(),
        criteria: vec![],
        description: None,
        required_capabilities: vec![],
        suggested_role: None,
        priority: 0,
    });

    let report = PlanValidator::validate(&proposal);
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("must specify at least one acceptance criterion")));
}

#[test]
fn test_self_dependency_rejection() {
    let mut proposal = PlanProposal::new("Self Dep", "Task depends on itself");
    proposal.tasks.push(ProposedTask {
        temp_id: "task-1".into(),
        objective: "Work".into(),
        criteria: vec!["file:work.txt".into()],
        description: None,
        required_capabilities: vec![],
        suggested_role: None,
        priority: 0,
    });
    proposal = proposal.with_dependency("task-1", "task-1");

    let report = PlanValidator::validate(&proposal);
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("cannot depend on itself")));
}

#[test]
fn test_dangling_dependency_rejection() {
    let mut proposal = PlanProposal::new("Dangling Dep", "References non-existent task");
    proposal.tasks.push(ProposedTask {
        temp_id: "task-1".into(),
        objective: "Work".into(),
        criteria: vec!["file:work.txt".into()],
        description: None,
        required_capabilities: vec![],
        suggested_role: None,
        priority: 0,
    });
    proposal = proposal.with_dependency("task-1", "task-ghost");

    let report = PlanValidator::validate(&proposal);
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("references non-existent prerequisite")));
}

#[tokio::test]
async fn test_plan_applier_validates_and_persists_inspectable_record() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let wf = Workflow::new("WF", "Autonomous objective");
    store.create_workflow(&wf).await.unwrap();

    let mut proposal = PlanProposal::new("Autonomous objective", "Rationale breakdown");
    proposal.tasks.push(ProposedTask {
        temp_id: "t1".into(),
        objective: "Create backend".into(),
        description: Some("Implement rust api".into()),
        criteria: vec!["file:backend.rs".into()],
        required_capabilities: vec!["filesystem_write".into()],
        suggested_role: Some("Backend Dev".into()),
        priority: 10,
    });
    proposal.tasks.push(ProposedTask {
        temp_id: "t2".into(),
        objective: "Create tests".into(),
        description: Some("Test rust api".into()),
        criteria: vec!["file:test_backend.rs".into()],
        required_capabilities: vec!["filesystem_write".into(), "shell_execute".into()],
        suggested_role: Some("Tester".into()),
        priority: 5,
    });
    proposal = proposal.with_dependency("t2", "t1");

    let applier = PlanApplier::new(store.clone());
    let ctx = PlanningContext::new(wf.id, "Autonomous objective");

    let res = applier
        .apply(
            &ctx,
            &proposal,
            "mock-provider",
            "mock-model",
            45,
            Some(10),
            Some(20),
        )
        .await
        .expect("apply valid plan");

    assert_eq!(res.created_tasks.len(), 2);
    assert_eq!(res.dependencies_created, 1);

    // Verify tasks exist in SQLite
    let t1_id = res.created_tasks.get("t1").unwrap();
    let t2_id = res.created_tasks.get("t2").unwrap();

    let t1 = store.get_task(t1_id).await.unwrap().unwrap();
    let t2 = store.get_task(t2_id).await.unwrap().unwrap();

    // t1 has no dependencies -> Ready; t2 depends on t1 -> Backlog
    assert_eq!(t1.state, TaskState::Ready);
    assert_eq!(t2.state, TaskState::Backlog);

    let t2_deps = store.get_dependencies(t2_id).await.unwrap();
    assert_eq!(t2_deps, vec![*t1_id]);

    // Verify inspectable persistent planning record
    let plan_record = store.get_plan_record(&res.plan_id).await.unwrap().unwrap();
    assert_eq!(plan_record.status, plexis_core::PlanStatus::Applied);
    assert_eq!(plan_record.latency_ms, 45);
    assert_eq!(plan_record.prompt_tokens, Some(10));
}

#[tokio::test]
async fn test_plan_applier_rejects_and_persists_rejected_record() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let wf = Workflow::new("WF", "Invalid objective");
    store.create_workflow(&wf).await.unwrap();

    let proposal = PlanProposal::new("Invalid objective", "No tasks");
    let applier = PlanApplier::new(store.clone());
    let ctx = PlanningContext::new(wf.id, "Invalid objective");

    let err = applier
        .apply(
            &ctx,
            &proposal,
            "mock-provider",
            "mock-model",
            10,
            None,
            None,
        )
        .await;

    assert!(err.is_err());

    // Verify persistent planning record recorded rejection
    let records = store.list_plan_records_by_workflow(&wf.id).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, plexis_core::PlanStatus::Rejected);
    assert!(!records[0].validation_errors.is_empty());
}

#[test]
fn test_failure_diagnoser_recommends_appropriate_actions() {
    let wf_id = WorkflowId::new();
    let mut task = Task::new(wf_id, "Failing Task").with_max_attempts(3);
    task.attempts = 1;

    // Missing artifact -> RetryWithFeedback
    let verif_missing = Verification {
        id: plexis_core::ids::VerificationId::new(),
        task_id: task.id,
        verifier_kind: "workspace_verifier".into(),
        verdict: VerificationVerdict::Failed,
        evidence: serde_json::json!({}),
        failure_reason: Some("missing file: src/math.rs".into()),
        duration_ms: 10,
        created_at: chrono::Utc::now(),
    };
    let rec1 = FailureDiagnoser::diagnose(&task, None, Some(&verif_missing));
    assert!(matches!(rec1.action, RecoveryAction::RetryWithFeedback(_)));

    // Permission denied -> EscalateHuman
    let mut exec = Execution::new(task.id, plexis_core::ids::AgentId::new(), 1);
    exec.error_message = Some("Permission denied: ShellExecute capability required".into());
    let rec2 = FailureDiagnoser::diagnose(&task, Some(&exec), None);
    assert!(matches!(rec2.action, RecoveryAction::EscalateHuman(_)));

    // Max attempts exhausted -> EscalateHuman
    task.attempts = 3;
    let rec3 = FailureDiagnoser::diagnose(&task, None, Some(&verif_missing));
    assert!(matches!(rec3.action, RecoveryAction::EscalateHuman(_)));
}
