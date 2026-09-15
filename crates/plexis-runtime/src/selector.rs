//! Capability-aware deterministic agent selection.
//!
//! Matches task requirements against available idle agents considering capabilities,
//! provider model capabilities, permissions, roles, and load with deterministic tie-breaking.

use plexis_core::state::AgentState;
use plexis_core::{Agent, Task};
use plexis_providers::capabilities::{
    standard_capability_matrix, ProviderCapabilities, ReasoningTier,
};

/// Deterministic capability-aware agent selector.
pub struct AgentSelector;

impl AgentSelector {
    /// Selects the optimal idle agent capable of executing `task` using the standard capability matrix.
    ///
    /// Returns `None` if no idle agent satisfies all required capabilities.
    pub fn select_best_agent<'a>(task: &Task, candidates: &'a [Agent]) -> Option<&'a Agent> {
        let matrix = standard_capability_matrix();
        Self::select_best_agent_with_matrix(task, candidates, &matrix)
    }

    /// Selects the optimal idle agent capable of executing `task` given an authoritative provider matrix.
    pub fn select_best_agent_with_matrix<'a>(
        task: &Task,
        candidates: &'a [Agent],
        matrix: &[ProviderCapabilities],
    ) -> Option<&'a Agent> {
        let required_caps = task.required_capabilities();
        let suggested_role = task
            .metadata
            .get("suggested_role")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase());

        let min_reasoning = task
            .metadata
            .get("min_reasoning_tier")
            .and_then(|v| v.as_str())
            .and_then(|s| match s.to_lowercase().as_str() {
                "none" => Some(ReasoningTier::None),
                "low" => Some(ReasoningTier::Low),
                "medium" => Some(ReasoningTier::Medium),
                "high" => Some(ReasoningTier::High),
                _ => None,
            })
            .unwrap_or(ReasoningTier::None);

        let requires_tools = task
            .metadata
            .get("requires_tools")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let requires_vision = task
            .metadata
            .get("requires_vision")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let required_context = task
            .metadata
            .get("required_context_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

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
            .filter(|a| {
                // If task specifies provider requirements, check agent's provider profile
                if min_reasoning > ReasoningTier::None
                    || requires_tools
                    || requires_vision
                    || required_context > 0
                {
                    let p_name = &a.provider_profile.provider;
                    let m_name = &a.provider_profile.model;
                    if let Some(caps) = matrix
                        .iter()
                        .find(|c| {
                            c.provider.eq_ignore_ascii_case(p_name)
                                && c.model.eq_ignore_ascii_case(m_name)
                        })
                        .or_else(|| {
                            matrix
                                .iter()
                                .find(|c| c.provider.eq_ignore_ascii_case(p_name))
                        })
                    {
                        if caps.reasoning_tier < min_reasoning {
                            return false;
                        }
                        if requires_tools && !caps.supports_tools {
                            return false;
                        }
                        if requires_vision && !caps.supports_vision {
                            return false;
                        }
                        if required_context > 0 && caps.context_window_tokens < required_context {
                            return false;
                        }
                    }
                }
                true
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
    let agent_role_lower = agent.role.to_lowercase();

    // 1. Exact or partial suggested role match (+50)
    if let Some(role) = suggested_role {
        if agent_role_lower.contains(role) || role.contains(&agent_role_lower) {
            score += 50;
        }
    }

    // 2. Specialized role domain affinities (+30)
    for cap in required_caps {
        let cap_lower = cap.to_lowercase();
        let matches_domain = ((cap_lower.contains("plan") || cap_lower.contains("architect"))
            && agent_role_lower.contains("plan"))
            || ((cap_lower.contains("research")
                || cap_lower.contains("search")
                || cap_lower.contains("read"))
                && agent_role_lower.contains("research"))
            || ((cap_lower.contains("write")
                || cap_lower.contains("code")
                || cap_lower.contains("develop"))
                && agent_role_lower.contains("develop"))
            || ((cap_lower.contains("test")
                || cap_lower.contains("runner")
                || cap_lower.contains("qa"))
                && agent_role_lower.contains("test"))
            || ((cap_lower.contains("review") || cap_lower.contains("diagnostic"))
                && agent_role_lower.contains("review"))
            || ((cap_lower.contains("integrat") || cap_lower.contains("git"))
                && agent_role_lower.contains("integrat"))
            || ((cap_lower.contains("verif") || cap_lower.contains("proof"))
                && agent_role_lower.contains("verif"));

        if matches_domain {
            score += 30;
        }
    }

    // 3. Exact capability match specialization (+10)
    // Penalize excess unneeded capabilities slightly to favor specialized agents over kitchen-sink agents
    if agent.capabilities.len() == required_caps.len() {
        score += 10;
    }

    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use plexis_core::ExecutionProfile;
    use serde_json::json;

    fn create_test_agent(
        name: &str,
        role: &str,
        provider: &str,
        model: &str,
        caps: &[&str],
    ) -> Agent {
        let profile = ExecutionProfile::new(provider, model);
        let mut agent = Agent::new(name, role, profile);
        agent.capabilities = caps.iter().map(|s| s.to_string()).collect();
        agent
    }

    #[test]
    fn test_select_specialized_agents_by_role_and_caps() {
        let planner = create_test_agent(
            "Architect",
            "Planner",
            "openai",
            "gpt-4o",
            &["planning", "filesystem_read"],
        );
        let researcher = create_test_agent(
            "Scout",
            "Researcher",
            "gemini",
            "gemini-1.5-pro",
            &["filesystem_read", "search"],
        );
        let developer = create_test_agent(
            "Dev",
            "Developer",
            "openai",
            "gpt-4o",
            &["filesystem_write", "shell"],
        );
        let tester = create_test_agent(
            "QA",
            "Tester",
            "ollama",
            "qwen2.5-coder",
            &["test_runner", "shell"],
        );
        let reviewer = create_test_agent(
            "Reviewer",
            "Reviewer",
            "openai",
            "gpt-4o",
            &["diff_review", "diagnostic"],
        );
        let integrator = create_test_agent(
            "Integrator",
            "Integrator",
            "openai",
            "gpt-4o",
            &["git", "integration"],
        );
        let verifier = create_test_agent(
            "Verifier",
            "Verifier",
            "openai",
            "gpt-4o",
            &["verification", "integration"],
        );

        let candidates = vec![
            planner.clone(),
            researcher.clone(),
            developer.clone(),
            tester.clone(),
            reviewer.clone(),
            integrator.clone(),
            verifier.clone(),
        ];

        // 1. Task requiring code development
        let mut dev_task = Task::new(
            plexis_core::ids::WorkflowId::new(),
            "Implement rate limiter",
        );
        dev_task.metadata = json!({
            "required_capabilities": ["filesystem_write", "shell"],
            "suggested_role": "Developer"
        });
        let selected = AgentSelector::select_best_agent(&dev_task, &candidates).unwrap();
        assert_eq!(selected.id, developer.id);

        // 2. Task requiring QA test running
        let mut test_task = Task::new(plexis_core::ids::WorkflowId::new(), "Run cargo test");
        test_task.metadata = json!({
            "required_capabilities": ["test_runner", "shell"],
            "suggested_role": "Tester"
        });
        let selected = AgentSelector::select_best_agent(&test_task, &candidates).unwrap();
        assert_eq!(selected.id, tester.id);

        // 3. Task requiring independent verification
        let mut verif_task = Task::new(plexis_core::ids::WorkflowId::new(), "Verify artifacts");
        verif_task.metadata = json!({
            "required_capabilities": ["verification"],
            "suggested_role": "Verifier"
        });
        let selected = AgentSelector::select_best_agent(&verif_task, &candidates).unwrap();
        assert_eq!(selected.id, verifier.id);
    }

    #[test]
    fn test_provider_capability_requirement_filtering() {
        let low_agent = create_test_agent(
            "FastBot",
            "Developer",
            "openai",
            "gpt-4o-mini",
            &["filesystem_write"],
        );
        let high_agent = create_test_agent(
            "ReasonBot",
            "Developer",
            "openai",
            "o1-preview",
            &["filesystem_write"],
        );

        let candidates = vec![low_agent.clone(), high_agent.clone()];

        // Task requires high reasoning tier
        let mut task = Task::new(
            plexis_core::ids::WorkflowId::new(),
            "Complex algorithm design",
        );
        task.metadata = json!({
            "required_capabilities": ["filesystem_write"],
            "min_reasoning_tier": "high"
        });

        let selected = AgentSelector::select_best_agent(&task, &candidates).unwrap();
        assert_eq!(selected.id, high_agent.id);
    }
}
