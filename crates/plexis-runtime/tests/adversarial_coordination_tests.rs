use std::sync::Arc;

use plexis_core::ids::{AgentId, TaskId, WorkflowId};
use plexis_core::state::TaskState;
use plexis_core::{
    Agent, ApprovalState, ExecutionProfile, Task, Verification, VerificationVerdict, Workflow,
};
use plexis_planner::{FailureDiagnoser, RecoveryAction};
use plexis_runtime::context::{ContextBudget, ContextBuilder};
use plexis_runtime::governance::GovernanceManager;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::selector::AgentSelector;
use plexis_storage::traits::{AgentStore, MessageStore, TaskStore, WorkflowStore};
use plexis_storage::SqliteStore;

#[tokio::test]
async fn test_duplicate_decomposition_prevention() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let wf = Workflow::new("WF", "Decomp test");
    store.create_workflow(&wf).await.unwrap();

    let parent = Task::new(wf.id, "Parent to decompose");
    let p_id = parent.id;
    store.create_task(&parent).await.unwrap();

    let c1 = Task::new(wf.id, "Child 1").with_parent(p_id);
    let c2 = Task::new(wf.id, "Child 2").with_parent(p_id);
    let c1_id = c1.id;
    let c2_id = c2.id;

    // First decomposition succeeds
    store
        .decompose_task_transactional(&p_id, &[c1, c2], &[(c2_id, c1_id)], &[c2_id])
        .await
        .expect("first decomposition succeeds");

    // Parent is now discarded
    let parent_after = store.get_task(&p_id).await.unwrap().unwrap();
    assert_eq!(parent_after.state, TaskState::Discarded);

    // Attempting to decompose non-existent or duplicate parent can be safely handled
    let ghost_id = TaskId::new();
    let err = store
        .decompose_task_transactional(&ghost_id, &[], &[], &[])
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_simultaneous_agent_assignment_lease_fencing() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let wf = Workflow::new("WF", "Lease test");
    store.create_workflow(&wf).await.unwrap();

    let task = Task::new(wf.id, "Contested Task");
    let task_id = task.id;
    store.create_task(&task).await.unwrap();

    let agent_1 = Agent::new("A1", "Worker", ExecutionProfile::new("mock", "model"));
    let agent_2 = Agent::new("A2", "Worker", ExecutionProfile::new("mock", "model"));
    store.create_agent(&agent_1).await.unwrap();
    store.create_agent(&agent_2).await.unwrap();

    let lease_mgr = LeaseManager::new(store.clone());

    // Agent 1 acquires lease
    let lease1 = lease_mgr
        .acquire(task_id, agent_1.id, chrono::Duration::minutes(5))
        .await
        .expect("agent 1 gets lease");

    // Agent 2 attempts to acquire lease simultaneously -> rejected
    let lease2_err = lease_mgr
        .acquire(task_id, agent_2.id, chrono::Duration::minutes(5))
        .await;
    assert!(
        lease2_err.is_err(),
        "Concurrent lease acquisition on same task must be rejected"
    );

    // After Agent 1 releases lease, Agent 2 can acquire
    lease_mgr.release(&lease1.id).await.unwrap();

    let lease2 = lease_mgr
        .acquire(task_id, agent_2.id, chrono::Duration::minutes(5))
        .await
        .expect("agent 2 gets lease after release");

    assert_eq!(lease2.agent_id, agent_2.id);
}

#[tokio::test]
async fn test_message_delivery_offline_recipient() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let wf = Workflow::new("WF", "Messaging test");
    store.create_workflow(&wf).await.unwrap();

    let sender = AgentId::new();
    let offline_receiver = AgentId::new();

    let msg = plexis_core::AgentMessage::new(
        sender,
        offline_receiver,
        wf.id,
        plexis_core::MessageType::Handoff,
        "Here is the database schema for the next phase",
    )
    .with_payload(serde_json::json!({"tables": ["users", "sessions"]}));

    // Message saved durably even though offline_receiver has no active session
    store.send_message(&msg).await.expect("send message");

    let inbox = store
        .list_messages_for_agent(&offline_receiver)
        .await
        .expect("query inbox");
    assert_eq!(inbox.len(), 1);
    assert_eq!(
        inbox[0].content,
        "Here is the database schema for the next phase"
    );
    assert_eq!(inbox[0].message_type, plexis_core::MessageType::Handoff);
}

#[tokio::test]
async fn test_human_approval_rejection_pathway() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let wf = Workflow::new("WF", "Approval test");
    store.create_workflow(&wf).await.unwrap();

    let mut task = Task::new(wf.id, "Destructive task");
    let _ = task.transition_to(TaskState::Ready);
    store.create_task(&task).await.unwrap();

    let gov = GovernanceManager::new(store.clone());

    // 1. Request approval
    let appr = gov
        .request_approval(
            task.id,
            wf.id,
            None,
            "Drop database tables",
            Some("Migration cleanup".into()),
        )
        .await
        .expect("request approval");

    let task_held = store.get_task(&task.id).await.unwrap().unwrap();
    assert_eq!(task_held.state, TaskState::NeedsHuman);

    // 2. Reject approval
    let rejected_appr = gov
        .submit_decision(&appr.id, false, Some("Too dangerous in production".into()))
        .await
        .expect("submit rejection");

    assert_eq!(rejected_appr.state, ApprovalState::Rejected);

    // Task transitions to Failed
    let task_failed = store.get_task(&task.id).await.unwrap().unwrap();
    assert_eq!(task_failed.state, TaskState::Failed);
}

#[tokio::test]
async fn test_human_approval_resume_pathway() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let wf = Workflow::new("WF", "Approval grant test");
    store.create_workflow(&wf).await.unwrap();

    let mut task = Task::new(wf.id, "Sensitive operation");
    let _ = task.transition_to(TaskState::Ready);
    store.create_task(&task).await.unwrap();

    let gov = GovernanceManager::new(store.clone());

    let appr = gov
        .request_approval(task.id, wf.id, None, "Deploy to staging", None)
        .await
        .unwrap();

    // Grant approval
    let approved = gov
        .submit_decision(&appr.id, true, Some("Authorized by lead".into()))
        .await
        .unwrap();
    assert_eq!(approved.state, ApprovalState::Approved);

    // Task is restored to Ready for execution
    let task_ready = store.get_task(&task.id).await.unwrap().unwrap();
    assert_eq!(task_ready.state, TaskState::Ready);
}

#[test]
fn test_context_truncation_under_strict_budget() {
    let agent = Agent::new("Worker", "Engineer", ExecutionProfile::new("mock", "model"));
    let task =
        Task::new(WorkflowId::new(), "Critical task").with_criteria(vec!["file:main.rs".into()]);

    let large_artifact_content = "A".repeat(10_000); // 10 KB artifact
    let many_messages: Vec<_> = (0..20)
        .map(|i| {
            plexis_core::AgentMessage::new(
                AgentId::new(),
                AgentId::new(),
                task.workflow_id,
                plexis_core::MessageType::Question,
                format!("Message #{}", i),
            )
        })
        .collect();

    let budget = ContextBudget {
        max_input_tokens: 2048,
        max_history_turns: 5,
        max_artifact_bytes: 1000,
        max_memories: 5,
    };

    let summary = ContextBuilder::new(&agent, "Objective", &task)
        .with_budget(budget)
        .with_dependency_summary("big_dep", &large_artifact_content)
        .with_incoming_messages(&many_messages)
        .build();

    // Verify older messages were omitted (20 total, max 5 turns -> 15 omitted)
    assert_eq!(summary.omitted_messages_count, 15);

    // Verify large artifact was truncated (10,000 - 1,000 = 9,000 bytes truncated)
    assert_eq!(summary.truncated_bytes, 9000);

    // Verify critical task criteria and system rules are still present in messages
    let user_msg = &summary.messages[1];
    let user_content = user_msg.content.as_deref().unwrap();
    assert!(user_content.contains("file:main.rs"));
    assert!(user_content.contains("[truncated: omitted 9000 bytes]"));
    assert!(user_content.contains("15 older messages omitted"));
}

#[test]
fn test_replanning_after_failure_with_diagnostics() {
    let task = Task::new(WorkflowId::new(), "Calculate primes")
        .with_criteria(vec!["file:primes.txt".into()]);

    let verif_fail = Verification {
        id: plexis_core::ids::VerificationId::new(),
        task_id: task.id,
        verifier_kind: "workspace_verifier".into(),
        verdict: VerificationVerdict::Failed,
        evidence: serde_json::json!({}),
        failure_reason: Some("missing file: primes.txt: file not found on disk".into()),
        duration_ms: 15,
        created_at: chrono::Utc::now(),
    };

    let rec = FailureDiagnoser::diagnose(&task, None, Some(&verif_fail));
    match rec.action {
        RecoveryAction::RetryWithFeedback(feedback) => {
            assert!(feedback.contains("primes.txt"));
            assert!(feedback.contains("Ensure the required files are created"));
        }
        other => panic!("Expected RetryWithFeedback, got {:?}", other),
    }
}

#[test]
fn test_agent_selector_capability_matching_and_scoring() {
    let task = Task::new(WorkflowId::new(), "Compile Rust code")
        .with_required_capabilities(vec!["filesystem_write".into(), "rust_compiler".into()]);

    let a1 = Agent::new("General", "Dev", ExecutionProfile::new("mock", "model"))
        .with_capabilities(vec!["filesystem_write".into()]); // Missing rust_compiler

    let a2 = Agent::new(
        "Rustacean",
        "Rust Specialist",
        ExecutionProfile::new("mock", "model"),
    )
    .with_capabilities(vec!["filesystem_write".into(), "rust_compiler".into()]); // Exact match

    let a3 = Agent::new(
        "Kitchen Sink",
        "Dev",
        ExecutionProfile::new("mock", "model"),
    )
    .with_capabilities(vec![
        "filesystem_write".into(),
        "rust_compiler".into(),
        "docker".into(),
        "k8s".into(),
    ]); // Has extra capabilities

    let candidates = vec![a1, a2.clone(), a3];
    let selected = AgentSelector::select_best_agent(&task, &candidates);

    assert!(selected.is_some());
    // a2 is selected because it satisfies all capabilities and has exact specialization score bonus
    assert_eq!(selected.unwrap().id, a2.id);
}
