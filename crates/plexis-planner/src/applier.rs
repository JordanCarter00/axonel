//! Applies validated planning proposals to durable task graphs and records inspectable state.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use plexis_core::ids::{PlanId, TaskId};
use plexis_core::state::TaskState;
use plexis_core::{Event, PlanningRecord, Task};
use plexis_storage::traits::{EventStore, PlanStore, TaskStore};

use crate::planner::{PlannerError, PlanningContext};
use crate::proposal::PlanProposal;
use crate::validator::PlanValidator;

/// Result of applying a validated plan to durable storage.
#[derive(Debug, Clone)]
pub struct PlanApplicationResult {
    /// ID of the persistent planning record.
    pub plan_id: PlanId,
    /// Created tasks mapped from their temporary IDs to durable TaskIds.
    pub created_tasks: HashMap<String, TaskId>,
    /// Number of dependency edges established.
    pub dependencies_created: usize,
}

/// Applies validated plan proposals transactionally to the durable task graph.
pub struct PlanApplier<S: TaskStore + PlanStore + EventStore + 'static> {
    store: Arc<S>,
}

impl<S: TaskStore + PlanStore + EventStore + 'static> PlanApplier<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// Validates and applies a planning proposal to the database.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply(
        &self,
        context: &PlanningContext,
        proposal: &PlanProposal,
        provider: &str,
        model: &str,
        latency_ms: u64,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    ) -> Result<PlanApplicationResult, PlannerError> {
        let proposal_json = serde_json::to_value(proposal)
            .map_err(|e| PlannerError::Serialization(e.to_string()))?;

        // 1. Validate proposal invariants
        let report = PlanValidator::validate(proposal);

        if !report.is_valid {
            let mut record = PlanningRecord::new(
                context.workflow_id,
                &context.objective,
                provider,
                model,
                proposal_json,
            )
            .with_validation(false, report.errors.clone())
            .with_telemetry(latency_ms, prompt_tokens, completion_tokens);

            if let Some(parent_id) = context.parent_task_id {
                record = record.with_parent_task(parent_id);
            }

            self.store
                .save_plan_record(&record)
                .await
                .map_err(|e| PlannerError::Storage(e.to_string()))?;

            let rej_evt = Event::new(
                "workflow",
                context.workflow_id.to_string(),
                "plan_rejected",
                serde_json::json!({
                    "plan_id": record.id.to_string(),
                    "errors": report.errors,
                }),
            );
            let _ = self.store.append_event(&rej_evt).await;

            return Err(PlannerError::Validation(report.errors));
        }

        // 2. Map temp_ids to durable TaskIds
        let mut id_map: HashMap<String, TaskId> = HashMap::new();
        let mut tasks: Vec<Task> = Vec::new();

        for pt in &proposal.tasks {
            let task_id = TaskId::new();
            id_map.insert(pt.temp_id.clone(), task_id);

            let mut task = Task::new(context.workflow_id, &pt.objective)
                .with_criteria(pt.criteria.clone())
                .with_priority(pt.priority);

            task.id = task_id;
            task.workspace_id = context.workspace_id;
            if let Some(desc) = &pt.description {
                task = task.with_description(desc);
            }
            if let Some(parent_id) = context.parent_task_id {
                task = task.with_parent(parent_id);
            }
            if !pt.required_capabilities.is_empty() {
                task = task.with_required_capabilities(pt.required_capabilities.clone());
            }
            tasks.push(task);
        }

        // 3. Map dependencies
        let mut child_deps: Vec<(TaskId, TaskId)> = Vec::new();
        for dep in &proposal.dependencies {
            if let (Some(&t_id), Some(&dep_id)) = (
                id_map.get(&dep.task_temp_id),
                id_map.get(&dep.depends_on_temp_id),
            ) {
                child_deps.push((t_id, dep_id));
            }
        }

        // 4. Persist to storage
        if let Some(parent_id) = context.parent_task_id {
            // Find terminal children: children that no other child in this decomposition depends on
            let internal_prerequisites: HashSet<TaskId> =
                child_deps.iter().map(|(_, dep)| *dep).collect();
            let terminal_children: Vec<TaskId> = tasks
                .iter()
                .map(|t| t.id)
                .filter(|id| !internal_prerequisites.contains(id))
                .collect();

            // Transactional decomposition in storage
            self.store
                .decompose_task_transactional(&parent_id, &tasks, &child_deps, &terminal_children)
                .await
                .map_err(|e| PlannerError::Storage(e.to_string()))?;
        } else {
            // Root-level workflow plan creation
            let dependent_tasks: HashSet<TaskId> = child_deps.iter().map(|(t, _)| *t).collect();

            for mut task in tasks {
                // If task has no dependencies, it is ready to execute immediately
                if !dependent_tasks.contains(&task.id) {
                    let _ = task.transition_to(TaskState::Ready);
                }
                self.store
                    .create_task(&task)
                    .await
                    .map_err(|e| PlannerError::Storage(e.to_string()))?;
            }

            for (task_id, depends_on_id) in &child_deps {
                self.store
                    .add_dependency(task_id, depends_on_id)
                    .await
                    .map_err(|e| PlannerError::Storage(e.to_string()))?;
            }
        }

        // 5. Persist inspectable planning record
        let applied_mutations = serde_json::json!({
            "created_task_count": id_map.len(),
            "created_tasks": id_map.iter().map(|(k, v)| (k.clone(), v.to_string())).collect::<HashMap<_, _>>(),
            "dependencies_count": child_deps.len(),
            "parent_task_id": context.parent_task_id.map(|id| id.to_string()),
        });

        let mut record = PlanningRecord::new(
            context.workflow_id,
            &context.objective,
            provider,
            model,
            proposal_json,
        )
        .with_validation(true, vec![])
        .mark_applied(applied_mutations)
        .with_telemetry(latency_ms, prompt_tokens, completion_tokens);

        if let Some(parent_id) = context.parent_task_id {
            record = record.with_parent_task(parent_id);
        }

        let plan_id = record.id;
        self.store
            .save_plan_record(&record)
            .await
            .map_err(|e| PlannerError::Storage(e.to_string()))?;

        // 6. Emit audit event
        let applied_evt = Event::new(
            "workflow",
            context.workflow_id.to_string(),
            "plan_applied",
            serde_json::json!({
                "plan_id": plan_id.to_string(),
                "created_task_count": id_map.len(),
                "dependencies_count": child_deps.len(),
                "parent_task_id": context.parent_task_id.map(|id| id.to_string()),
            }),
        );
        let _ = self.store.append_event(&applied_evt).await;

        Ok(PlanApplicationResult {
            plan_id,
            created_tasks: id_map,
            dependencies_created: child_deps.len(),
        })
    }
}
