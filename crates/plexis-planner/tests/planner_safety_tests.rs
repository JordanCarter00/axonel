use plexis_planner::{PlanProposal, PlanValidator, PlannerBudgets, ProposedTask};

fn make_task(id: &str) -> ProposedTask {
    ProposedTask {
        temp_id: id.to_string(),
        objective: format!("Objective for {}", id),
        description: None,
        criteria: vec!["file:test.txt".to_string()],
        required_capabilities: vec![],
        suggested_role: None,
        priority: 0,
    }
}

#[test]
fn test_planner_budget_exceeded_tasks_per_plan() {
    let budget = PlannerBudgets {
        max_tasks_per_plan: 3,
        max_workflow_tasks: 10,
        max_decomposition_depth: 5,
    };

    let mut proposal = PlanProposal::new("Objective", "Rationale");
    proposal.tasks = vec![
        make_task("t1"),
        make_task("t2"),
        make_task("t3"),
        make_task("t4"),
    ];

    let report = PlanValidator::validate_with_budget(&proposal, &budget, 0);
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("exceeds maximum task budget of 3")));
}

#[test]
fn test_planner_budget_exceeded_workflow_tasks_limit() {
    let budget = PlannerBudgets {
        max_tasks_per_plan: 10,
        max_workflow_tasks: 5,
        max_decomposition_depth: 5,
    };

    let mut proposal = PlanProposal::new("Objective", "Rationale");
    proposal.tasks = vec![make_task("t1"), make_task("t2"), make_task("t3")];

    // Current workflow already has 3 tasks, adding 3 brings it to 6 > 5
    let report = PlanValidator::validate_with_budget(&proposal, &budget, 3);
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("exceeds maximum workflow task limit of 5")));
}

#[test]
fn test_planner_budget_exceeded_decomposition_depth() {
    let budget = PlannerBudgets {
        max_tasks_per_plan: 10,
        max_workflow_tasks: 20,
        max_decomposition_depth: 3,
    };

    // Chain: t1 -> t2 -> t3 -> t4 (depth is 4 > 3)
    let proposal = PlanProposal::new("Objective", "Rationale")
        .with_task(make_task("t1"))
        .with_task(make_task("t2"))
        .with_task(make_task("t3"))
        .with_task(make_task("t4"))
        .with_dependency("t2", "t1")
        .with_dependency("t3", "t2")
        .with_dependency("t4", "t3");

    let report = PlanValidator::validate_with_budget(&proposal, &budget, 0);
    assert!(!report.is_valid);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("decomposition depth of 4 exceeds maximum allowed depth of 3")));
}

#[test]
fn test_planner_budget_within_limits_passes() {
    let budget = PlannerBudgets {
        max_tasks_per_plan: 5,
        max_workflow_tasks: 10,
        max_decomposition_depth: 3,
    };

    let proposal = PlanProposal::new("Objective", "Rationale")
        .with_task(make_task("t1"))
        .with_task(make_task("t2"))
        .with_dependency("t2", "t1");

    let report = PlanValidator::validate_with_budget(&proposal, &budget, 2);
    assert!(
        report.is_valid,
        "Valid proposal within budget must pass: {:?}",
        report.errors
    );
}
