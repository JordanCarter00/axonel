//! Mission liveness, progress measurement, and stagnation detection.

use chrono::{DateTime, Utc};
use plexis_core::mission::MissionHealth;

/// Measurable observable physical and task progress indicators.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub completed_tasks_count: usize,
    pub task_graph_node_count: usize,
    pub latest_git_commit: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl ProgressSnapshot {
    pub fn new(
        completed_tasks_count: usize,
        task_graph_node_count: usize,
        latest_git_commit: Option<String>,
    ) -> Self {
        Self {
            completed_tasks_count,
            task_graph_node_count,
            latest_git_commit,
            timestamp: Utc::now(),
        }
    }

    /// Evaluates if measurable observable progress occurred since the previous snapshot.
    pub fn has_progressed_from(&self, previous: &ProgressSnapshot) -> bool {
        self.completed_tasks_count > previous.completed_tasks_count
            || self.task_graph_node_count > previous.task_graph_node_count
            || (self.latest_git_commit.is_some()
                && self.latest_git_commit != previous.latest_git_commit)
    }
}

/// Evaluates health and liveness of a mission based on observable progress.
#[derive(Debug, Clone)]
pub struct LivenessEvaluator {
    pub last_progress: ProgressSnapshot,
    pub consecutive_stagnant_cycles: u32,
    pub max_stagnant_cycles: u32,
}

impl LivenessEvaluator {
    pub fn new(max_stagnant_cycles: u32, initial_snapshot: ProgressSnapshot) -> Self {
        Self {
            last_progress: initial_snapshot,
            consecutive_stagnant_cycles: 0,
            max_stagnant_cycles,
        }
    }

    /// Evaluates the current progress snapshot.
    /// Returns `(health, has_progressed)`.
    pub fn evaluate(&mut self, current: ProgressSnapshot) -> (MissionHealth, bool) {
        let progressed = current.has_progressed_from(&self.last_progress);
        if progressed {
            self.consecutive_stagnant_cycles = 0;
            self.last_progress = current;
            (MissionHealth::Healthy, true)
        } else {
            self.consecutive_stagnant_cycles += 1;
            if self.consecutive_stagnant_cycles >= self.max_stagnant_cycles {
                (MissionHealth::Stagnant, false)
            } else {
                (MissionHealth::Degraded, false)
            }
        }
    }
}
