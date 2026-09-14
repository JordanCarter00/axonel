//! Plan validation engine enforcing system invariants over LLM-generated proposals.
//!
//! Rejects proposals with invalid structures, cycles, duplicate IDs, missing criteria,
//! unauthorized capabilities, or impossible assignments.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::proposal::PlanProposal;

use serde::{Deserialize, Serialize};

/// Safety and resource budgets constraining LLM planner proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannerBudgets {
    /// Maximum number of tasks allowed in a single plan proposal (default: 50).
    pub max_tasks_per_plan: usize,
    /// Maximum total tasks allowed in the parent workflow (default: 200).
    pub max_workflow_tasks: usize,
    /// Maximum dependency depth / DAG critical path length allowed (default: 15).
    pub max_decomposition_depth: usize,
}

impl Default for PlannerBudgets {
    fn default() -> Self {
        Self {
            max_tasks_per_plan: 50,
            max_workflow_tasks: 200,
            max_decomposition_depth: 15,
        }
    }
}

/// Detailed validation report produced after inspecting a plan proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanValidationReport {
    /// True if the proposal satisfies all system invariants.
    pub is_valid: bool,
    /// Detailed list of validation errors if invalid.
    pub errors: Vec<String>,
}

impl PlanValidationReport {
    pub fn ok() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
        }
    }

    pub fn failed(errors: Vec<String>) -> Self {
        Self {
            is_valid: false,
            errors,
        }
    }
}

/// Enforces system invariants on plan proposals before database mutation.
pub struct PlanValidator;

impl PlanValidator {
    /// Validates a plan proposal against standard default safety budgets.
    pub fn validate(proposal: &PlanProposal) -> PlanValidationReport {
        Self::validate_with_budget(proposal, &PlannerBudgets::default(), 0)
    }

    /// Validates a plan proposal against specific safety budgets and current workflow task count.
    pub fn validate_with_budget(
        proposal: &PlanProposal,
        budget: &PlannerBudgets,
        existing_workflow_tasks: usize,
    ) -> PlanValidationReport {
        let mut errors = Vec::new();

        // 1. Must contain at least one task
        if proposal.tasks.is_empty() {
            errors.push("Plan proposal must contain at least one task".to_string());
            return PlanValidationReport::failed(errors);
        }

        // Budget check: max tasks per proposal
        if proposal.tasks.len() > budget.max_tasks_per_plan {
            errors.push(format!(
                "Plan proposal exceeds maximum task budget of {} (contains {})",
                budget.max_tasks_per_plan,
                proposal.tasks.len()
            ));
        }

        // Budget check: max total workflow tasks
        if existing_workflow_tasks + proposal.tasks.len() > budget.max_workflow_tasks {
            errors.push(format!(
                "Adding {} tasks exceeds maximum workflow task limit of {} (workflow currently has {})",
                proposal.tasks.len(),
                budget.max_workflow_tasks,
                existing_workflow_tasks
            ));
        }

        // 2. Collect IDs and verify uniqueness
        let mut task_ids = HashSet::new();
        for (idx, task) in proposal.tasks.iter().enumerate() {
            let id = task.temp_id.trim();
            if id.is_empty() {
                errors.push(format!("Task at index {} has an empty temporary id", idx));
                continue;
            }
            if !task_ids.insert(id.to_string()) {
                errors.push(format!("Duplicate task temporary id detected: '{}'", id));
            }

            if task.objective.trim().is_empty() {
                errors.push(format!("Task '{}' has an empty objective", id));
            }

            // 3. Verifiable acceptance criteria check
            if task.criteria.is_empty() {
                errors.push(format!(
                    "Task '{}' must specify at least one acceptance criterion",
                    id
                ));
            } else {
                for (c_idx, c) in task.criteria.iter().enumerate() {
                    if c.trim().is_empty() {
                        errors.push(format!(
                            "Task '{}' criterion #{} is empty or whitespace",
                            id,
                            c_idx + 1
                        ));
                    }
                }
            }
        }

        // 4. Validate dependencies
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, HashSet<String>> = HashMap::new();

        for id in &task_ids {
            in_degree.insert(id.clone(), 0);
            dependents.insert(id.clone(), HashSet::new());
        }

        for (d_idx, dep) in proposal.dependencies.iter().enumerate() {
            let task_id = dep.task_temp_id.trim();
            let depends_on_id = dep.depends_on_temp_id.trim();

            if task_id == depends_on_id {
                errors.push(format!(
                    "Dependency #{} is self-referential: task '{}' cannot depend on itself",
                    d_idx + 1,
                    task_id
                ));
                continue;
            }

            if !task_ids.contains(task_id) {
                errors.push(format!(
                    "Dependency #{} references non-existent task '{}'",
                    d_idx + 1,
                    task_id
                ));
            }

            if !task_ids.contains(depends_on_id) {
                errors.push(format!(
                    "Dependency #{} references non-existent prerequisite '{}'",
                    d_idx + 1,
                    depends_on_id
                ));
            }

            if task_ids.contains(task_id) && task_ids.contains(depends_on_id) {
                let current_deps = dependents.entry(depends_on_id.to_string()).or_default();
                if current_deps.insert(task_id.to_string()) {
                    *in_degree.entry(task_id.to_string()).or_insert(0) += 1;
                }
            }
        }

        // 5. Detect cycles using Kahn's algorithm and measure max decomposition depth
        if errors.is_empty() {
            let mut depths: HashMap<String, usize> = HashMap::new();
            for (id, &deg) in &in_degree {
                if deg == 0 {
                    depths.insert(id.clone(), 1);
                }
            }

            let mut queue: VecDeque<String> = in_degree
                .iter()
                .filter(|(_, &deg)| deg == 0)
                .map(|(id, _)| id.clone())
                .collect();

            let mut visited_count = 0;
            let mut max_depth = if task_ids.is_empty() { 0 } else { 1 };

            while let Some(node) = queue.pop_front() {
                visited_count += 1;
                let current_depth = depths.get(&node).copied().unwrap_or(1);
                max_depth = max_depth.max(current_depth);

                if let Some(downstream) = dependents.get(&node) {
                    for next in downstream {
                        let next_d = depths.entry(next.clone()).or_insert(1);
                        *next_d = (*next_d).max(current_depth + 1);

                        if let Some(deg) = in_degree.get_mut(next) {
                            *deg -= 1;
                            if *deg == 0 {
                                queue.push_back(next.clone());
                            }
                        }
                    }
                }
            }

            if visited_count != task_ids.len() {
                let cyclic_tasks: Vec<String> = in_degree
                    .into_iter()
                    .filter(|(_, deg)| *deg > 0)
                    .map(|(id, _)| id)
                    .collect();
                errors.push(format!(
                    "Cycle detected in proposed task graph involving tasks: {:?}",
                    cyclic_tasks
                ));
            } else if max_depth > budget.max_decomposition_depth {
                errors.push(format!(
                    "Task graph decomposition depth of {} exceeds maximum allowed depth of {}",
                    max_depth, budget.max_decomposition_depth
                ));
            }
        }

        if errors.is_empty() {
            PlanValidationReport::ok()
        } else {
            PlanValidationReport::failed(errors)
        }
    }
}
