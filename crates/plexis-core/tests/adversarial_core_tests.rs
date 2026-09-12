use plexis_core::ids::{AgentId, TaskId, WorkflowId};
use plexis_core::state::{AgentState, CommandState, ExecutionState, TaskState, WorkflowState};
use plexis_core::{GraphError, Lease, Task, TaskGraph};

#[test]
fn test_adversarial_self_dependency_rejected() {
    let wf = WorkflowId::new();
    let mut graph = TaskGraph::new();

    let t1 = Task::new(wf, "Self-referencing task");
    let id1 = t1.id;
    graph.add_task(t1);

    // Self-loop must be rejected
    let err = graph.add_dependency(id1, id1);
    assert!(matches!(err, Err(GraphError::CycleDetected(id)) if id == id1));
    assert_eq!(graph.direct_dependencies(&id1).len(), 0);
}

#[test]
fn test_adversarial_deep_cyclic_loop_prevention() {
    let wf = WorkflowId::new();
    let mut graph = TaskGraph::new();

    let tasks: Vec<Task> = (0..10)
        .map(|i| Task::new(wf, format!("Node {i}")))
        .collect();
    let ids: Vec<TaskId> = tasks.iter().map(|t| t.id).collect();

    for t in tasks {
        graph.add_task(t);
    }

    // Connect in a chain: 0 -> 1 -> 2 -> ... -> 9
    for i in 1..10 {
        graph.add_dependency(ids[i], ids[i - 1]).unwrap();
    }

    // Attempt to close the loop: 0 depends on 9 (creating a 10-node cycle)
    let cycle_err = graph.add_dependency(ids[0], ids[9]);
    assert!(cycle_err.is_err());
    assert!(matches!(cycle_err, Err(GraphError::CycleDetected(_))));

    // Graph must remain completely intact and acyclic
    assert!(graph.validate_acyclic().is_ok());
    let order = graph.topological_sort().unwrap();
    assert_eq!(order, ids);
}

#[test]
fn test_adversarial_task_removal_safety() {
    let wf = WorkflowId::new();
    let mut graph = TaskGraph::new();

    let parent = Task::new(wf, "Upstream");
    let child = Task::new(wf, "Downstream");
    let p_id = parent.id;
    let c_id = child.id;

    graph.add_task(parent);
    graph.add_task(child);
    graph.add_dependency(c_id, p_id).unwrap();

    // Removing upstream task while downstream depends on it must fail
    let remove_err = graph.remove_task(&p_id);
    assert!(matches!(
        remove_err,
        Err(GraphError::InvalidDecomposition(_))
    ));

    // Downstream still depends on upstream
    assert!(graph.contains_task(&p_id));
    assert!(graph.direct_dependencies(&c_id).contains(&p_id));

    // Removing downstream first is allowed
    assert!(graph.remove_task(&c_id).is_ok());
    assert!(!graph.contains_task(&c_id));

    // Now removing upstream succeeds
    assert!(graph.remove_task(&p_id).is_ok());
    assert!(!graph.contains_task(&p_id));
}

#[test]
fn test_adversarial_state_transition_matrix() {
    // 1. TaskState illegal jumps
    let mut task_state = TaskState::Backlog;
    assert!(task_state.transition_to(TaskState::Verified).is_err());
    assert!(task_state.transition_to(TaskState::Running).is_err());
    assert!(task_state
        .transition_to(TaskState::AwaitingVerification)
        .is_err());

    let mut discarded = TaskState::Discarded;
    assert!(discarded.transition_to(TaskState::Ready).is_err());
    assert!(discarded.transition_to(TaskState::Backlog).is_err());

    // 2. AgentState illegal transitions
    let mut agent_state = AgentState::Terminated;
    assert!(agent_state.transition_to(AgentState::Idle).is_err());
    assert!(agent_state.transition_to(AgentState::Busy).is_err());

    // 3. WorkflowState illegal transitions
    let mut wf_state = WorkflowState::Completed;
    assert!(wf_state.transition_to(WorkflowState::Active).is_err());
    assert!(wf_state.transition_to(WorkflowState::Draft).is_err());

    // 4. ExecutionState illegal transitions
    let mut exec_state = ExecutionState::Completed;
    assert!(exec_state.transition_to(ExecutionState::Running).is_err());
    assert!(exec_state.transition_to(ExecutionState::Pending).is_err());

    // 5. CommandState illegal transitions
    let mut cmd_state = CommandState::Confirmed;
    assert!(cmd_state.transition_to(CommandState::Queued).is_err());
    assert!(cmd_state.transition_to(CommandState::Dispatched).is_err());
}

#[test]
fn test_adversarial_stale_fencing_token_validation() {
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let now = chrono::Utc::now();
    let mut lease = Lease::new(task_id, agent_id, chrono::Duration::hours(1));

    assert_eq!(lease.generation, 1);
    assert!(lease.validate_token(1, now).is_ok());

    // Simulate 4 successive reassignments
    for gen in 2..=5 {
        lease.reassign(AgentId::new(), chrono::Duration::hours(1), now);
        assert_eq!(lease.generation, gen);

        // Every prior generation must be rejected
        for old_gen in 1..gen {
            let err = lease.validate_token(old_gen, now);
            assert!(err.is_err());
        }
        // Current generation passes
        assert!(lease.validate_token(gen, now).is_ok());
    }
}
