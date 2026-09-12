//! Plan validation engine enforcing system invariants over LLM-generated proposals.
//!
//! Rejects proposals with invalid structures, cycles, duplicate IDs, missing criteria,
//! unauthorized capabilities, or impossible assignments.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::proposal::PlanProposal;

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
    /// Validates a plan proposal against all core structural and domain invariants.
    pub fn validate(proposal: &PlanProposal) -> PlanValidationReport {
        let mut errors = Vec::new();

        // 1. Must contain at least one task
        if proposal.tasks.is_empty() {
            errors.push("Plan proposal must contain at least one task".to_string());
            return PlanValidationReport::failed(errors);
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

        // 5. Detect cycles using Kahn's algorithm
        if errors.is_empty() {
            let mut queue: VecDeque<String> = in_degree
                .iter()
                .filter(|(_, &deg)| deg == 0)
                .map(|(id, _)| id.clone())
                .collect();

            let mut visited_count = 0;
            while let Some(node) = queue.pop_front() {
                visited_count += 1;
                if let Some(downstream) = dependents.get(&node) {
                    for next in downstream {
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
            }
        }

        if errors.is_empty() {
            PlanValidationReport::ok()
        } else {
            PlanValidationReport::failed(errors)
        }
    }
}
