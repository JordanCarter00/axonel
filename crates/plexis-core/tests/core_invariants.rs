use chrono::Duration;
use plexis_core::*;

#[test]
fn test_identifier_type_safety_and_serialization() {
    let task_id = TaskId::new();
    let agent_id = AgentId::new();
    let wf_id = WorkflowId::new();

    let task_str = task_id.to_string();
    assert!(task_str.starts_with("task_"));

    let parsed_task: TaskId = task_str.parse().expect("parse task id");
    assert_eq!(task_id, parsed_task);

    // Serialization roundtrip
    let serialized = serde_json::to_string(&task_id).unwrap();
    let deserialized: TaskId = serde_json::from_str(&serialized).unwrap();
    assert_eq!(task_id, deserialized);

    // Ensure distinct type prefixes
    assert_eq!(TaskId::prefix(), "task");
    assert_eq!(AgentId::prefix(), "agent");
    assert_eq!(WorkflowId::prefix(), "wf");
    assert_ne!(task_id.to_string(), agent_id.to_string());
    assert_ne!(task_id.to_string(), wf_id.to_string());
}

#[test]
fn test_task_state_transition_invariants() {
    let mut task = Task::new(WorkflowId::new(), "Implement parser");
    assert_eq!(task.state, TaskState::Backlog);

    // Legal forward progression
    assert!(task.transition_to(TaskState::Ready).is_ok());
    assert!(task.transition_to(TaskState::Assigned).is_ok());
    assert!(task.transition_to(TaskState::Running).is_ok());
    assert!(task.transition_to(TaskState::AwaitingVerification).is_ok());
    assert!(task.transition_to(TaskState::Verified).is_ok());

    // Disallowed transition from terminal state
    assert!(task.transition_to(TaskState::Ready).is_err());
    assert!(task.transition_to(TaskState::Running).is_err());
}

#[test]
fn test_task_failure_recovery_pathway() {
    let mut task = Task::new(WorkflowId::new(), "Execute migration");
    task.transition_to(TaskState::Ready).unwrap();
    task.transition_to(TaskState::Assigned).unwrap();
    task.transition_to(TaskState::Running).unwrap();

    // Failure during run
    assert!(task.transition_to(TaskState::Failed).is_ok());
    assert_eq!(task.state, TaskState::Failed);

    // Transition to retrying with recovery context
    assert!(task.transition_to(TaskState::Retrying).is_ok());
    assert_eq!(task.state, TaskState::Retrying);

    // Back to ready for rescheduling
    assert!(task.transition_to(TaskState::Ready).is_ok());
    assert_eq!(task.state, TaskState::Ready);
}

#[test]
fn test_task_assignment_under_lease() {
    let mut task = Task::new(WorkflowId::new(), "Review security");
    let agent_id = AgentId::new();

    // Assigning to agent automatically transitions to Assigned
    assert!(task.assign_to(agent_id).is_ok());
    assert_eq!(task.state, TaskState::Assigned);
    assert_eq!(task.assigned_agent_id, Some(agent_id));

    // Unassign returns to Ready
    assert!(task.unassign().is_ok());
    assert_eq!(task.state, TaskState::Ready);
    assert_eq!(task.assigned_agent_id, None);
}

#[test]
fn test_lease_fencing_token_semantics() {
    let task_id = TaskId::new();
    let agent_1 = AgentId::new();
    let agent_2 = AgentId::new();

    let mut lease = Lease::new(task_id, agent_1, Duration::minutes(5));
    assert_eq!(lease.generation, 1);

    // Worker with valid token passes
    assert!(lease.validate_token(1, chrono::Utc::now()).is_ok());

    // Stale worker token (e.g. 0 or 2) is rejected
    assert!(lease.validate_token(0, chrono::Utc::now()).is_err());
    assert!(lease.validate_token(2, chrono::Utc::now()).is_err());

    // Reclaiming/reassigning increments generation
    lease.reassign(agent_2, Duration::minutes(5), chrono::Utc::now());
    assert_eq!(lease.generation, 2);
    assert_eq!(lease.agent_id, agent_2);

    // Old worker 1 with generation 1 is now fenced out
    assert!(lease.validate_token(1, chrono::Utc::now()).is_err());
    // New worker 2 with generation 2 succeeds
    assert!(lease.validate_token(2, chrono::Utc::now()).is_ok());
}

#[test]
fn test_task_graph_diamond_dag_and_cycle_prevention() {
    let wf = WorkflowId::new();
    let mut graph = TaskGraph::new();

    // Diamond: A -> B, A -> C, B -> D, C -> D
    // (D depends on B and C; B and C depend on A)
    let t_a = Task::new(wf, "Task A");
    let t_b = Task::new(wf, "Task B");
    let t_c = Task::new(wf, "Task C");
    let t_d = Task::new(wf, "Task D");

    let id_a = t_a.id;
    let id_b = t_b.id;
    let id_c = t_c.id;
    let id_d = t_d.id;

    graph.add_task(t_a);
    graph.add_task(t_b);
    graph.add_task(t_c);
    graph.add_task(t_d);

    graph.add_dependency(id_b, id_a).unwrap();
    graph.add_dependency(id_c, id_a).unwrap();
    graph.add_dependency(id_d, id_b).unwrap();
    graph.add_dependency(id_d, id_c).unwrap();

    let sorted = graph.topological_sort().unwrap();
    // A must come first, D must come last
    assert_eq!(sorted[0], id_a);
    assert_eq!(sorted[3], id_d);

    // Attempting to make A depend on D creates a cycle
    let cycle_result = graph.add_dependency(id_a, id_d);
    assert!(cycle_result.is_err());
    assert!(matches!(cycle_result, Err(GraphError::CycleDetected(_))));

    // Graph remains intact and acyclic
    assert!(graph.validate_acyclic().is_ok());
}

#[test]
fn test_task_decomposition_rewiring() {
    let wf = WorkflowId::new();
    let mut graph = TaskGraph::new();

    // Root -> Parent -> Final
    let root = Task::new(wf, "Root");
    let parent = Task::new(wf, "Parent to decompose");
    let final_task = Task::new(wf, "Final dependent");

    let root_id = root.id;
    let parent_id = parent.id;
    let final_id = final_task.id;

    graph.add_task(root);
    graph.add_task(parent);
    graph.add_task(final_task);

    graph.add_dependency(parent_id, root_id).unwrap();
    graph.add_dependency(final_id, parent_id).unwrap();

    // Decompose Parent into Child 1 -> Child 2
    let c1 = Task::new(wf, "Child 1").with_parent(parent_id);
    let c2 = Task::new(wf, "Child 2").with_parent(parent_id);
    let c1_id = c1.id;
    let c2_id = c2.id;

    graph
        .decompose_task(
            parent_id,
            vec![c1, c2],
            vec![(c2_id, c1_id)], // c2 depends on c1
            vec![c2_id],          // terminal child is c2
        )
        .expect("decomposition succeeds");

    // c1 should have inherited root dependency
    assert!(graph.direct_dependencies(&c1_id).contains(&root_id));
    // c2 should depend on c1
    assert!(graph.direct_dependencies(&c2_id).contains(&c1_id));
    // final should now depend on terminal child c2
    assert!(graph.direct_dependencies(&final_id).contains(&c2_id));

    let order = graph.topological_sort().unwrap();
    let pos = |id: TaskId| order.iter().position(|&x| x == id).unwrap();

    assert!(pos(root_id) < pos(c1_id));
    assert!(pos(c1_id) < pos(c2_id));
    assert!(pos(c2_id) < pos(final_id));
}
