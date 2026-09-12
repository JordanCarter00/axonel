//! Capability-aware deterministic agent selection.
//!
//! Matches task requirements against available idle agents considering capabilities,
//! permissions, roles, and load with deterministic tie-breaking.

use plexis_core::state::AgentState;
use plexis_core::{Agent, Task};

/// Deterministic capability-aware agent selector.
pub struct AgentSelector;

impl AgentSelector {
    /// Selects the optimal idle agent capable of executing `task`.
    ///
    /// Returns `None` if no idle agent satisfies all required capabilities.
    pub fn select_best_agent<'a>(task: &Task, candidates: &'a [Agent]) -> Option<&'a Agent> {
        let required_caps = task.required_capabilities();
        let suggested_role = task
            .metadata
            .get("suggested_role")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase());

        // 1. Filter candidates
        let mut eligible: Vec<&'a Agent> = candidates
            .iter()
            .filter(|a| a.state == AgentState::Idle && a.current_execution_id.is_none())
            .filter(|a| {
                // Agent must have all required capabilities
                required_caps
                    .iter()
                    .all(|req| a.capabilities.iter().any(|c| c.eq_ignore_ascii_case(req)))
            })
            .collect();

        if eligible.is_empty() {
            return None;
        }

        // 2. Score candidates deterministically
        eligible.sort_by(|a, b| {
            let score_a = score_agent(a, &required_caps, suggested_role.as_deref());
            let score_b = score_agent(b, &required_caps, suggested_role.as_deref());

            score_b.cmp(&score_a).then_with(|| a.id.cmp(&b.id)) // Deterministic tie breaker
        });

        eligible.into_iter().next()
    }
}

fn score_agent(agent: &Agent, required_caps: &[String], suggested_role: Option<&str>) -> i32 {
    let mut score = 0;

    // Suggested role match (+50)
    if let Some(role) = suggested_role {
        if agent.role.to_lowercase().contains(role) || role.contains(&agent.role.to_lowercase()) {
            score += 50;
        }
    }

    // Exact capability match specialization: penalize excess unneeded capabilities slightly
    // to favor specialized agents over kitchen-sink agents (+10 for exact count match)
    if agent.capabilities.len() == required_caps.len() {
        score += 10;
    }

    score
}
