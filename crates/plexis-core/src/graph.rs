//! First-class Task Graph abstraction for Plexis.
//!
//! Represents the directed dependency structure of tasks within a workflow.
//! Provides deterministic dependency resolution, cycle detection, topological sorting,
//! runnable task discovery, and safe decomposition primitives.
//!
//! The graph is an ephemeral queryable view that is always 100% reconstructible
//! from durable database state.

use crate::ids::TaskId;
use crate::state::TaskState;
use crate::task::Task;
use std::collections::{HashMap, HashSet, VecDeque};

/// Errors arising during graph manipulation or queries.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    #[error("task '{0}' not found in task graph")]
    TaskNotFound(TaskId),
    #[error("cycle detected in task graph involving task '{0}'")]
    CycleDetected(TaskId),
    #[error("invalid decomposition: {0}")]
    InvalidDecomposition(String),
}

/// In-memory queryable directed acyclic graph (DAG) of tasks.
#[derive(Debug, Clone, Default)]
pub struct TaskGraph {
    tasks: HashMap<TaskId, Task>,
    /// Maps a task to the set of tasks it directly depends on (upstream tasks).
    dependencies: HashMap<TaskId, HashSet<TaskId>>,
    /// Maps a task to the set of tasks that depend on it (downstream dependents).
    dependents: HashMap<TaskId, HashSet<TaskId>>,
}

impl TaskGraph {
    /// Creates a new empty task graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a task to the graph.
    pub fn add_task(&mut self, task: Task) {
        let id = task.id;
        self.tasks.insert(id, task);
        self.dependencies.entry(id).or_default();
        self.dependents.entry(id).or_default();
    }

    /// Adds a directed dependency: `task_id` depends on `depends_on_id`.
    ///
    /// This means `depends_on_id` must be completed/verified before `task_id` can run.
    pub fn add_dependency(
        &mut self,
        task_id: TaskId,
        depends_on_id: TaskId,
    ) -> Result<(), GraphError> {
        if task_id == depends_on_id {
            return Err(GraphError::CycleDetected(task_id));
        }
        if !self.tasks.contains_key(&task_id) {
            return Err(GraphError::TaskNotFound(task_id));
        }
        if !self.tasks.contains_key(&depends_on_id) {
            return Err(GraphError::TaskNotFound(depends_on_id));
        }

        self.dependencies
            .entry(task_id)
            .or_default()
            .insert(depends_on_id);
        self.dependents
            .entry(depends_on_id)
            .or_default()
            .insert(task_id);

        // Verify that adding this edge did not introduce a cycle
        if let Err(err) = self.validate_acyclic() {
            // Revert edge on cycle detection
            self.dependencies
                .entry(task_id)
                .or_default()
                .remove(&depends_on_id);
            self.dependents
                .entry(depends_on_id)
                .or_default()
                .remove(&task_id);
            return Err(err);
        }

        Ok(())
    }

    /// Removes a directed dependency between two tasks.
    pub fn remove_dependency(
        &mut self,
        task_id: TaskId,
        depends_on_id: TaskId,
    ) -> Result<(), GraphError> {
        if !self.tasks.contains_key(&task_id) {
            return Err(GraphError::TaskNotFound(task_id));
        }
        if !self.tasks.contains_key(&depends_on_id) {
            return Err(GraphError::TaskNotFound(depends_on_id));
        }

        if let Some(deps) = self.dependencies.get_mut(&task_id) {
            deps.remove(&depends_on_id);
        }
        if let Some(dependents) = self.dependents.get_mut(&depends_on_id) {
            dependents.remove(&task_id);
        }

        Ok(())
    }

    /// Checks if a task exists in the graph.
    pub fn contains_task(&self, task_id: &TaskId) -> bool {
        self.tasks.contains_key(task_id)
    }

    /// Removes a task from the graph if it has no downstream dependents.
    pub fn remove_task(&mut self, task_id: &TaskId) -> Result<Option<Task>, GraphError> {
        if let Some(downstream) = self.dependents.get(task_id) {
            if !downstream.is_empty() {
                return Err(GraphError::InvalidDecomposition(format!(
                    "cannot remove task '{}' because other tasks still depend on it",
                    task_id
                )));
            }
        }
        // Remove incoming dependency edges
        if let Some(deps) = self.dependencies.remove(task_id) {
            for dep in deps {
                if let Some(set) = self.dependents.get_mut(&dep) {
                    set.remove(task_id);
                }
            }
        }
        self.dependents.remove(task_id);
        Ok(self.tasks.remove(task_id))
    }

    /// Retrieves a reference to a task in the graph.
    pub fn get_task(&self, task_id: &TaskId) -> Option<&Task> {
        self.tasks.get(task_id)
    }

    /// Retrieves a mutable reference to a task in the graph.
    pub fn get_task_mut(&mut self, task_id: &TaskId) -> Option<&mut Task> {
        self.tasks.get_mut(task_id)
    }

    /// Direct dependencies that `task_id` must wait for.
    pub fn direct_dependencies(&self, task_id: &TaskId) -> HashSet<TaskId> {
        self.dependencies.get(task_id).cloned().unwrap_or_default()
    }

    /// Downstream tasks directly waiting on `task_id`.
    pub fn direct_dependents(&self, task_id: &TaskId) -> HashSet<TaskId> {
        self.dependents.get(task_id).cloned().unwrap_or_default()
    }

    /// Returns the total number of tasks in the graph.
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    /// Validates that the graph is a strict DAG with no cycles using Kahn's algorithm.
    pub fn validate_acyclic(&self) -> Result<(), GraphError> {
        // Calculate in-degree based on dependencies count
        let mut in_degree: HashMap<TaskId, usize> = HashMap::new();
        for &id in self.tasks.keys() {
            let count = self.dependencies.get(&id).map(|s| s.len()).unwrap_or(0);
            in_degree.insert(id, count);
        }

        let mut queue: VecDeque<TaskId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut visited_count = 0;

        while let Some(node) = queue.pop_front() {
            visited_count += 1;

            if let Some(downstream) = self.dependents.get(&node) {
                for &next in downstream {
                    if let Some(deg) = in_degree.get_mut(&next) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(next);
                        }
                    }
                }
            }
        }

        if visited_count != self.tasks.len() {
            // Find a task involved in a cycle
            for (&id, &deg) in &in_degree {
                if deg > 0 {
                    return Err(GraphError::CycleDetected(id));
                }
            }
        }

        Ok(())
    }

    /// Produces a topological ordering of tasks from upstream prerequisites to downstream dependents.
    pub fn topological_sort(&self) -> Result<Vec<TaskId>, GraphError> {
        let mut in_degree: HashMap<TaskId, usize> = HashMap::new();
        for &id in self.tasks.keys() {
            let count = self.dependencies.get(&id).map(|s| s.len()).unwrap_or(0);
            in_degree.insert(id, count);
        }

        let mut queue: VecDeque<TaskId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut order = Vec::with_capacity(self.tasks.len());

        while let Some(node) = queue.pop_front() {
            order.push(node);

            if let Some(downstream) = self.dependents.get(&node) {
                for &next in downstream {
                    if let Some(deg) = in_degree.get_mut(&next) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(next);
                        }
                    }
                }
            }
        }

        if order.len() != self.tasks.len() {
            return Err(GraphError::CycleDetected(
                order.last().copied().unwrap_or_default(),
            ));
        }

        Ok(order)
    }

    /// Finds all tasks that are currently runnable.
    ///
    /// A task is runnable if:
    /// 1. It is in a runnable candidate state (`Backlog` or `Ready`).
    /// 2. It is not currently assigned to an agent.
    /// 3. All of its direct dependencies are satisfied (i.e. in `Verified` state).
    ///
    /// Results are sorted by priority (descending, highest first), then creation timestamp.
    pub fn find_runnable_tasks(&self) -> Vec<TaskId> {
        let mut runnable = Vec::new();

        for (id, task) in &self.tasks {
            if !task.state.is_runnable_candidate() || task.assigned_agent_id.is_some() {
                continue;
            }

            let deps = self.dependencies.get(id);
            let all_deps_verified = match deps {
                Some(deps_set) => deps_set.iter().all(|dep_id| {
                    self.tasks
                        .get(dep_id)
                        .map(|dep_task| dep_task.state == TaskState::Verified)
                        .unwrap_or(false)
                }),
                None => true,
            };

            if all_deps_verified {
                runnable.push(task);
            }
        }

        // Sort by priority descending, then created_at ascending
        runnable.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.created_at.cmp(&b.created_at))
        });

        runnable.into_iter().map(|t| t.id).collect()
    }

    /// Decomposes a parent task into child tasks, safely linking them in the graph.
    ///
    /// Upstream dependencies of the parent are preserved, and downstream dependents of the
    /// parent will depend on the designated terminal child tasks.
    pub fn decompose_task(
        &mut self,
        parent_id: TaskId,
        children: Vec<Task>,
        child_dependencies: Vec<(TaskId, TaskId)>,
        terminal_child_ids: Vec<TaskId>,
    ) -> Result<(), GraphError> {
        if !self.tasks.contains_key(&parent_id) {
            return Err(GraphError::TaskNotFound(parent_id));
        }

        let existing_parent_deps = self.direct_dependencies(&parent_id);
        let existing_parent_dependents = self.direct_dependents(&parent_id);

        // Add all children
        for child in children {
            if child.parent_id != Some(parent_id) {
                return Err(GraphError::InvalidDecomposition(format!(
                    "child task '{}' parent_id must match '{}'",
                    child.id, parent_id
                )));
            }
            self.add_task(child);
        }

        // Add internal child-to-child dependencies
        for (task_id, depends_on) in child_dependencies {
            self.add_dependency(task_id, depends_on)?;
        }

        // Initial children (those with no internal dependencies) inherit the parent's dependencies
        let child_ids: Vec<TaskId> = self
            .tasks
            .values()
            .filter(|t| t.parent_id == Some(parent_id))
            .map(|t| t.id)
            .collect();

        for child_id in child_ids {
            if self.direct_dependencies(&child_id).is_empty() {
                for &p_dep in &existing_parent_deps {
                    self.add_dependency(child_id, p_dep)?;
                }
            }
        }

        // Downstream tasks that depended on the parent now depend on the terminal children
        for &dep_on_parent in &existing_parent_dependents {
            self.remove_dependency(dep_on_parent, parent_id)?;
            for &term_id in &terminal_child_ids {
                self.add_dependency(dep_on_parent, term_id)?;
            }
        }

        // Validate complete graph acyclic invariant
        self.validate_acyclic()?;

        Ok(())
    }

    /// Inserts a new task between an upstream predecessor and a downstream dependent.
    ///
    /// If `downstream_id` directly depended on `upstream_id`, that edge is removed.
    /// The new task will depend on `upstream_id`, and `downstream_id` will depend on `new_task`.
    pub fn insert_task_between(
        &mut self,
        new_task: Task,
        upstream_id: TaskId,
        downstream_id: TaskId,
    ) -> Result<(), GraphError> {
        if !self.tasks.contains_key(&upstream_id) {
            return Err(GraphError::TaskNotFound(upstream_id));
        }
        if !self.tasks.contains_key(&downstream_id) {
            return Err(GraphError::TaskNotFound(downstream_id));
        }

        let new_id = new_task.id;
        self.add_task(new_task);

        // Remove direct dependency between downstream and upstream if it exists
        let _ = self.remove_dependency(downstream_id, upstream_id);

        // Add upstream -> new_task and new_task -> downstream
        self.add_dependency(new_id, upstream_id)?;
        self.add_dependency(downstream_id, new_id)?;

        // Validate graph remains acyclic
        self.validate_acyclic()?;

        Ok(())
    }

    /// Replaces a parent task with child tasks, safely rewiring dependencies and removing the parent.
    pub fn replace_task_with_children(
        &mut self,
        parent_id: TaskId,
        children: Vec<Task>,
        child_dependencies: Vec<(TaskId, TaskId)>,
        terminal_child_ids: Vec<TaskId>,
    ) -> Result<(), GraphError> {
        self.decompose_task(parent_id, children, child_dependencies, terminal_child_ids)?;
        self.remove_task(&parent_id)?;
        Ok(())
    }

    /// Reassigns agent ownership for a task in the graph.
    pub fn reassign_ownership(
        &mut self,
        task_id: TaskId,
        new_agent_id: Option<crate::ids::AgentId>,
    ) -> Result<(), GraphError> {
        let task = self
            .tasks
            .get_mut(&task_id)
            .ok_or(GraphError::TaskNotFound(task_id))?;
        task.assigned_agent_id = new_agent_id;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::WorkflowId;

    #[test]
    fn test_empty_graph() {
        let graph = TaskGraph::new();
        assert_eq!(graph.task_count(), 0);
        assert!(graph.validate_acyclic().is_ok());
    }

    #[test]
    fn test_linear_dependencies_and_topological_sort() {
        let wf = WorkflowId::new();
        let mut graph = TaskGraph::new();

        let t1 = Task::new(wf, "Step 1");
        let t2 = Task::new(wf, "Step 2");
        let t3 = Task::new(wf, "Step 3");

        let id1 = t1.id;
        let id2 = t2.id;
        let id3 = t3.id;

        graph.add_task(t1);
        graph.add_task(t2);
        graph.add_task(t3);

        // t2 depends on t1, t3 depends on t2
        assert!(graph.add_dependency(id2, id1).is_ok());
        assert!(graph.add_dependency(id3, id2).is_ok());

        let order = graph.topological_sort().expect("should sort acyclic graph");
        assert_eq!(order, vec![id1, id2, id3]);
    }

    #[test]
    fn test_cycle_detection_rejects_edge() {
        let wf = WorkflowId::new();
        let mut graph = TaskGraph::new();

        let t1 = Task::new(wf, "A");
        let t2 = Task::new(wf, "B");
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_task(t1);
        graph.add_task(t2);

        assert!(graph.add_dependency(id2, id1).is_ok());
        // Introducing id1 depends on id2 creates a cycle: A -> B -> A
        let err = graph.add_dependency(id1, id2);
        assert!(matches!(err, Err(GraphError::CycleDetected(_))));

        // Graph remains acyclic after rejection
        assert!(graph.validate_acyclic().is_ok());
    }

    #[test]
    fn test_runnable_task_discovery() {
        let wf = WorkflowId::new();
        let mut graph = TaskGraph::new();

        let mut t1 = Task::new(wf, "Root 1").with_priority(10);
        let t2 = Task::new(wf, "Dependent 2").with_priority(50);
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_task(t1.clone());
        graph.add_task(t2);
        graph.add_dependency(id2, id1).unwrap();

        // Initially, t1 is runnable (no dependencies), t2 is blocked by t1
        let runnable = graph.find_runnable_tasks();
        assert_eq!(runnable, vec![id1]);

        // When t1 becomes verified, t2 becomes runnable
        t1.transition_to(TaskState::Ready).unwrap();
        t1.transition_to(TaskState::Assigned).unwrap();
        t1.transition_to(TaskState::Running).unwrap();
        t1.transition_to(TaskState::AwaitingVerification).unwrap();
        t1.transition_to(TaskState::Verified).unwrap();
        *graph.get_task_mut(&id1).unwrap() = t1;

        let runnable_after = graph.find_runnable_tasks();
        assert_eq!(runnable_after, vec![id2]);
    }

    #[test]
    fn test_insert_task_between() {
        let wf = WorkflowId::new();
        let mut graph = TaskGraph::new();

        let t1 = Task::new(wf, "Upstream");
        let t2 = Task::new(wf, "Downstream");
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_task(t1);
        graph.add_task(t2);
        graph.add_dependency(id2, id1).unwrap();

        let intermediate = Task::new(wf, "Intermediate");
        let id_mid = intermediate.id;

        graph.insert_task_between(intermediate, id1, id2).unwrap();

        // id_mid depends on id1, id2 depends on id_mid
        assert_eq!(
            graph.direct_dependencies(&id_mid),
            [id1].into_iter().collect()
        );
        assert_eq!(
            graph.direct_dependencies(&id2),
            [id_mid].into_iter().collect()
        );
        assert_eq!(graph.topological_sort().unwrap(), vec![id1, id_mid, id2]);
    }

    #[test]
    fn test_replace_task_with_children() {
        let wf = WorkflowId::new();
        let mut graph = TaskGraph::new();

        let parent = Task::new(wf, "Parent");
        let p_id = parent.id;
        graph.add_task(parent);

        let downstream = Task::new(wf, "Downstream");
        let d_id = downstream.id;
        graph.add_task(downstream);
        graph.add_dependency(d_id, p_id).unwrap();

        let c1 = Task::new(wf, "Child 1").with_parent(p_id);
        let c2 = Task::new(wf, "Child 2").with_parent(p_id);
        let c1_id = c1.id;
        let c2_id = c2.id;

        graph
            .replace_task_with_children(p_id, vec![c1, c2], vec![(c2_id, c1_id)], vec![c2_id])
            .unwrap();

        assert!(!graph.contains_task(&p_id));
        assert!(graph.contains_task(&c1_id));
        assert!(graph.contains_task(&c2_id));
        assert_eq!(
            graph.direct_dependencies(&d_id),
            [c2_id].into_iter().collect()
        );
    }

    #[test]
    fn test_reassign_ownership() {
        let wf = WorkflowId::new();
        let mut graph = TaskGraph::new();
        let task = Task::new(wf, "Work");
        let tid = task.id;
        graph.add_task(task);

        let agent = crate::ids::AgentId::new();
        assert!(graph.reassign_ownership(tid, Some(agent)).is_ok());
        assert_eq!(graph.get_task(&tid).unwrap().assigned_agent_id, Some(agent));
    }
}
