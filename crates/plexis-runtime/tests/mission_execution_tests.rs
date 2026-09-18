use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

use async_trait::async_trait;
use plexis_core::ids::{MissionId, WorkflowId};
use plexis_core::mission::{MissionBudget, StoppingCondition};
use plexis_core::state::{MissionState, TaskState};
use plexis_core::{Task, Workspace};
use plexis_runtime::error::RuntimeError;
use plexis_runtime::mission::{MissionEngine, WorkflowExecutionSummary, WorkflowExecutor};
use plexis_storage::sqlite::SqliteStore;
use plexis_storage::traits::{
    MissionStore, TaskStore, WorkflowStore, WorkspaceStore,
};

/// Mock WorkflowExecutor for validating cycle progression and engine dispatching.
struct MockWorkflowExecutor {
    pub call_count: AtomicUsize,
    pub fail_first_cycle: bool,
    pub in_progress: bool,
    pub store: Arc<SqliteStore>,
}

#[async_trait]
impl WorkflowExecutor for MockWorkflowExecutor {
    async fn execute_workflow_cycle(
        &self,
        workflow_id: &WorkflowId,
        _mission_id: &MissionId,
    ) -> Result<WorkflowExecutionSummary, RuntimeError> {
        let calls = self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut summary = WorkflowExecutionSummary::default();

        if self.fail_first_cycle && calls == 0 {
            // Simulate task failure in cycle 0
            let mut failed_task = Task::new(*workflow_id, "Failing Task");
            failed_task.state = TaskState::Failed;
            self.store.create_task(&failed_task).await.unwrap();

            summary.executed_tasks_count = 1;
            summary.failed_tasks_count = 1;
            summary.completed_tasks_count = 0;
        } else if self.in_progress {
            // Simulate task in progress (non-terminal)
            let mut t1 = Task::new(*workflow_id, "In Progress Task");
            t1.state = TaskState::Running;
            self.store.create_task(&t1).await.unwrap();

            summary.executed_tasks_count = 1;
            summary.completed_tasks_count = 0;
        } else {
            // Simulate successful completed task
            let mut t1 = Task::new(*workflow_id, "Executed Task");
            t1.state = TaskState::Verified;
            self.store.create_task(&t1).await.unwrap();

            summary.executed_tasks_count = 1;
            summary.completed_tasks_count = 1;
            summary.discovered_tasks_count = 1;
        }

        Ok(summary)
    }
}

#[tokio::test]
async fn test_mission_starts_workflow_and_dispatches_task() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let mock_executor = Arc::new(MockWorkflowExecutor {
        call_count: AtomicUsize::new(0),
        fail_first_cycle: false,
        in_progress: true,
        store: store.clone(),
    });

    let engine = MissionEngine::new(store.clone()).with_workflow_executor(mock_executor.clone());

    let mission = engine
        .create_mission(
            "Dispatch Objective",
            "Verify real workflow dispatch from mission engine",
            None,
            Some(MissionBudget::default()),
            Some(StoppingCondition::default()),
        )
        .await
        .unwrap();

    let started = engine.start_mission(mission.id).await.unwrap();
    assert_eq!(started.state, MissionState::Planning);

    let stepped = engine.step_mission(mission.id).await.unwrap();

    // Verify active workflow was created and executor was called
    assert!(stepped.active_workflow_id.is_some());
    assert_eq!(stepped.state, MissionState::Running);
    assert_eq!(mock_executor.call_count.load(Ordering::SeqCst), 1);
    assert_eq!(stepped.budget_consumed.total_executions, 1);

    // Verify workflow exists in store
    let wf = store
        .get_workflow(&stepped.active_workflow_id.unwrap())
        .await
        .unwrap();
    assert!(wf.is_some());
}

#[tokio::test]
async fn test_cycle_completes_only_after_actual_work() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());

    struct IdleThenActiveExecutor {
        pub call_count: AtomicUsize,
        pub store: Arc<SqliteStore>,
    }

    #[async_trait]
    impl WorkflowExecutor for IdleThenActiveExecutor {
        async fn execute_workflow_cycle(
            &self,
            workflow_id: &WorkflowId,
            _mission_id: &MissionId,
        ) -> Result<WorkflowExecutionSummary, RuntimeError> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            let mut summary = WorkflowExecutionSummary::default();
            if count == 0 {
                // Incomplete, 0 tasks executed
                let mut task = Task::new(*workflow_id, "In progress");
                task.state = TaskState::Running;
                self.store.create_task(&task).await.unwrap();
                summary.executed_tasks_count = 0;
            } else {
                let mut task = Task::new(*workflow_id, "Completed task");
                task.state = TaskState::Running;
                self.store.create_task(&task).await.unwrap();
                summary.executed_tasks_count = 2;
                summary.completed_tasks_count = 0;
            }
            Ok(summary)
        }
    }

    let executor = Arc::new(IdleThenActiveExecutor {
        call_count: AtomicUsize::new(0),
        store: store.clone(),
    });
    let engine = MissionEngine::new(store.clone()).with_workflow_executor(executor.clone());

    let mission = engine
        .create_mission(
            "Progressive Work Mission",
            "Ensure cycles advance only upon actual work",
            None,
            Some(MissionBudget::default()),
            Some(StoppingCondition::default()),
        )
        .await
        .unwrap();

    let _ = engine.start_mission(mission.id).await.unwrap();

    // Step 1: 0 executed tasks -> 0 consumed executions recorded
    let step1 = engine.step_mission(mission.id).await.unwrap();
    assert_eq!(step1.budget_consumed.total_executions, 0);
    assert_eq!(step1.cycle_index, 0);

    // Step 2: 2 executed tasks -> 2 consumed executions recorded
    let step2 = engine.step_mission(mission.id).await.unwrap();
    assert_eq!(step2.budget_consumed.total_executions, 2);
}

#[tokio::test]
async fn test_failed_workflow_causes_mission_replanning() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let executor = Arc::new(MockWorkflowExecutor {
        call_count: AtomicUsize::new(0),
        fail_first_cycle: true,
        in_progress: true,
        store: store.clone(),
    });

    let engine = MissionEngine::new(store.clone()).with_workflow_executor(executor.clone());

    let mission = engine
        .create_mission(
            "Adaptive Replan Mission",
            "Verify replanning on failure",
            None,
            Some(MissionBudget::default()),
            Some(StoppingCondition::default()),
        )
        .await
        .unwrap();

    let _ = engine.start_mission(mission.id).await.unwrap();

    // Step 1: Cycle 0 fails -> initiates replanning
    let step1 = engine.step_mission(mission.id).await.unwrap();
    assert_eq!(step1.state, MissionState::Replanning);
    assert_eq!(step1.cycle_index, 1);
    assert!(step1.active_workflow_id.is_none());

    // Verify cycle journal in storage
    let cycles = store.list_cycles(&mission.id).await.unwrap();
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].cycle_index, 0);
    assert_eq!(cycles[0].phase, "recovery");

    // Step 2: Adaptive replanned cycle 1 runs
    let step2 = engine.step_mission(mission.id).await.unwrap();
    assert!(step2.active_workflow_id.is_some());
    assert_eq!(step2.state, MissionState::Running);

    // Verify cycle 1 workflow received failure diagnostics from cycle 0
    let wf = store
        .get_workflow(&step2.active_workflow_id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        wf.metadata.get("cycle_index").and_then(|v| v.as_u64()),
        Some(1)
    );
    assert!(wf.metadata.get("failure_diagnostics").is_some());
}

#[tokio::test]
async fn test_successful_verification_causes_mission_completion() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize real Git repository
    Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Plexis Test"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@plexis.local"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    std::fs::write(repo_path.join("README.md"), "Initial repo\n").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let ws = Workspace::new("Test WS", repo_path.to_path_buf());
    store.create_workspace(&ws).await.unwrap();

    // Stopping condition requires clean working tree and valid git commit
    let cond = StoppingCondition {
        required_tests_pass: false,
        working_tree_clean: true,
        required_commit_exists: true,
        custom_verifier: None,
    };

    let executor = Arc::new(MockWorkflowExecutor {
        call_count: AtomicUsize::new(0),
        fail_first_cycle: false,
        in_progress: false,
        store: store.clone(),
    });
    let engine = MissionEngine::new(store.clone()).with_workflow_executor(executor);

    let mission = engine
        .create_mission(
            "Verified Completion Mission",
            "Verify stopping condition satisfied on disk",
            Some(ws.id),
            Some(MissionBudget::default()),
            Some(cond),
        )
        .await
        .unwrap();

    let _ = engine.start_mission(mission.id).await.unwrap();

    // Agent executes work on disk: creates new commit
    std::fs::write(repo_path.join("lib.rs"), "pub fn solution() {}\n").unwrap();
    Command::new("git")
        .args(["add", "lib.rs"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "feat: implement solution autonomously"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    let new_sha = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_path)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();

    // Step mission: evaluates stopping condition on disk
    let completed = engine.step_mission(mission.id).await.unwrap();

    assert_eq!(completed.state, MissionState::Completed);
    assert_eq!(completed.latest_verified_commit, Some(new_sha.clone()));
    let outcome = completed.final_outcome.expect("final outcome recorded");
    assert!(outcome.success);
    assert_eq!(outcome.verified_commit_sha, Some(new_sha));

    // Verify completion checkpoint
    let ckpt = engine
        .checkpoint_manager()
        .get_latest_checkpoint(&mission.id)
        .await
        .unwrap();
    assert!(ckpt.is_some());
}
