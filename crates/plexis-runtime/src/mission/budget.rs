//! Resource and operational budget tracking and bounds checking for missions.

use plexis_core::mission::{MissionBudget, MissionBudgetConsumed};

/// Helper managing consumption and limits for active missions.
#[derive(Debug, Clone)]
pub struct BudgetTracker {
    pub budget: MissionBudget,
    pub consumed: MissionBudgetConsumed,
}

impl BudgetTracker {
    pub fn new(budget: MissionBudget, consumed: MissionBudgetConsumed) -> Self {
        Self { budget, consumed }
    }

    pub fn record_execution(&mut self) {
        self.consumed.total_executions += 1;
    }

    pub fn record_duration(&mut self, elapsed_secs: u64) {
        self.consumed.duration_secs += elapsed_secs;
    }

    pub fn record_recovery(&mut self) {
        self.consumed.recovery_attempts += 1;
    }

    pub fn record_planner_iteration(&mut self) {
        self.consumed.planner_iterations += 1;
    }

    pub fn record_stagnant_cycle(&mut self) {
        self.consumed.stagnant_cycles += 1;
    }

    pub fn reset_stagnant_cycles(&mut self) {
        self.consumed.stagnant_cycles = 0;
    }

    pub fn check_exhaustion(&self) -> Option<String> {
        self.consumed.has_exhausted(&self.budget)
    }
}
