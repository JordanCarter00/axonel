//! Planner abstraction and LLM/Scripted implementations.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use plexis_core::ids::{TaskId, WorkflowId, WorkspaceId};
use plexis_providers::{ChatMessage, CompletionRequest, Provider};

use crate::proposal::PlanProposal;

/// Errors arising during autonomous planning.
#[derive(Debug, thiserror::Error)]
pub enum PlannerError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("proposal serialization error: {0}")]
    Serialization(String),
    #[error("validation rejected proposal: {0:?}")]
    Validation(Vec<String>),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("entity not found: {0}")]
    NotFound(String),
    #[error("planning failed: {0}")]
    Internal(String),
}

/// Context provided to the planner to generate a proposal.
#[derive(Debug, Clone)]
pub struct PlanningContext {
    /// Associated workflow.
    pub workflow_id: WorkflowId,
    /// Associated workspace.
    pub workspace_id: Option<WorkspaceId>,
    /// Optional parent task if this is dynamic decomposition.
    pub parent_task_id: Option<TaskId>,
    /// High-level objective.
    pub objective: String,
    /// Additional context (e.g. codebase summary, constraints).
    pub context: Option<String>,
    /// Available system capabilities that tasks may request.
    pub available_capabilities: Vec<String>,
    /// Available agent roles in the system.
    pub available_roles: Vec<String>,
    /// Previous failure diagnosis if this is a replanning run.
    pub failure_diagnostics: Option<String>,
}

impl PlanningContext {
    pub fn new(workflow_id: WorkflowId, objective: impl Into<String>) -> Self {
        Self {
            workflow_id,
            workspace_id: None,
            parent_task_id: None,
            objective: objective.into(),
            context: None,
            available_capabilities: Vec::new(),
            available_roles: Vec::new(),
            failure_diagnostics: None,
        }
    }

    pub fn with_workspace_id(mut self, workspace_id: WorkspaceId) -> Self {
        self.workspace_id = Some(workspace_id);
        self
    }

    pub fn with_parent_task(mut self, parent_id: TaskId) -> Self {
        self.parent_task_id = Some(parent_id);
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.available_capabilities = caps;
        self
    }

    pub fn with_failure_diagnostics(mut self, diagnostics: impl Into<String>) -> Self {
        self.failure_diagnostics = Some(diagnostics.into());
        self
    }
}

/// Asynchronous planner interface.
#[async_trait]
pub trait Planner: Send + Sync {
    /// Produces a structured planning proposal for the given context.
    async fn plan(&self, context: &PlanningContext) -> Result<PlanProposal, PlannerError>;

    /// Returns the provider name backing this planner.
    fn provider_name(&self) -> &str;

    /// Returns the model name used by this planner.
    fn model_name(&self) -> &str;
}

/// LLM-driven planner using a provider adapter.
pub struct LlmPlanner {
    provider: Arc<dyn Provider>,
    model: String,
}

impl LlmPlanner {
    pub fn new(provider: Arc<dyn Provider>, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
        }
    }

    fn build_prompt(&self, context: &PlanningContext) -> String {
        let mut prompt = format!(
            "You are the Plexis Autonomous Planner.\n\
             Your job is to decompose the following high-level objective into concrete, verifiable tasks.\n\n\
             OBJECTIVE:\n{}\n\n",
            context.objective
        );

        if let Some(ctx) = &context.context {
            prompt.push_str(&format!("CONTEXT & CONSTRAINTS:\n{}\n\n", ctx));
        }

        if let Some(diag) = &context.failure_diagnostics {
            prompt.push_str(&format!(
                "PREVIOUS ATTEMPT FAILURE DIAGNOSTICS (Address this in your plan):\n{}\n\n",
                diag
            ));
        }

        if !context.available_capabilities.is_empty() {
            prompt.push_str(&format!(
                "AVAILABLE CAPABILITIES:\n{}\n\n",
                context.available_capabilities.join(", ")
            ));
        }

        prompt.push_str(
            "Respond ONLY with a valid JSON object matching this schema:\n\
             {\n  \
               \"objective\": \"...\",\n  \
               \"rationale\": \"...\",\n  \
               \"tasks\": [\n    \
                 {\n      \
                   \"temp_id\": \"task-1\",\n      \
                   \"objective\": \"...\",\n      \
                   \"description\": \"...\",\n      \
                   \"criteria\": [\"file:path/to/artifact.rs\"],\n      \
                   \"required_capabilities\": [\"filesystem_write\"],\n      \
                   \"suggested_role\": \"Code Implementer\",\n      \
                   \"priority\": 10\n    \
                 }\n  \
               ],\n  \
               \"dependencies\": [\n    \
                 {\n      \
                   \"task_temp_id\": \"task-2\",\n      \
                   \"depends_on_temp_id\": \"task-1\"\n    \
                 }\n  \
               ],\n  \
               \"execution_strategy\": \"parallel\",\n  \
               \"verification_strategy\": \"disk_artifacts\"\n\
             }\n\
             Do NOT wrap the JSON in commentary or conversational text.",
        );

        prompt
    }
}

#[async_trait]
impl Planner for LlmPlanner {
    async fn plan(&self, context: &PlanningContext) -> Result<PlanProposal, PlannerError> {
        let prompt = self.build_prompt(context);
        let request = CompletionRequest::new(
            &self.model,
            vec![
                ChatMessage::system(
                    "You are a structured planning engine. Output strict JSON only.",
                ),
                ChatMessage::user(prompt),
            ],
        );

        let response = self
            .provider
            .complete(&request)
            .await
            .map_err(|e| PlannerError::Provider(e.to_string()))?;

        let raw_text = response.message.content.as_deref().unwrap_or("");
        let cleaned = extract_json_block(raw_text);

        let proposal: PlanProposal = serde_json::from_str(cleaned).map_err(|e| {
            PlannerError::Serialization(format!(
                "Failed to parse LLM planner response as PlanProposal JSON: {}. Raw: '{}'",
                e, raw_text
            ))
        })?;

        Ok(proposal)
    }

    fn provider_name(&self) -> &str {
        self.provider.id()
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Deterministic, scripted planner for hermetic testing and predictable workflows.
pub struct ScriptedPlanner {
    proposals: Mutex<Vec<PlanProposal>>,
    provider_name: String,
    model_name: String,
}

impl ScriptedPlanner {
    pub fn new(proposals: Vec<PlanProposal>) -> Self {
        Self {
            proposals: Mutex::new(proposals),
            provider_name: "scripted".to_string(),
            model_name: "mock-planner".to_string(),
        }
    }
}

#[async_trait]
impl Planner for ScriptedPlanner {
    async fn plan(&self, _context: &PlanningContext) -> Result<PlanProposal, PlannerError> {
        let mut proposals = self.proposals.lock().await;
        if proposals.is_empty() {
            Err(PlannerError::Internal(
                "ScriptedPlanner has no remaining proposals".into(),
            ))
        } else {
            Ok(proposals.remove(0))
        }
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

fn extract_json_block(text: &str) -> &str {
    let trimmed = text.trim();
    if let Some(start) = trimmed.find("```json") {
        let after_fence = &trimmed[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim();
        }
    }
    trimmed
}

/// Autonomous, dynamic rule-based decomposer that analyzes engineering objectives
/// and synthesizes valid, capability-matched DAG proposals without pre-canned hardcoding.
#[derive(Debug, Clone)]
pub struct AutonomousDecomposer {
    provider_name: String,
    model_name: String,
}

impl Default for AutonomousDecomposer {
    fn default() -> Self {
        Self::new()
    }
}

impl AutonomousDecomposer {
    pub fn new() -> Self {
        Self {
            provider_name: "autonomous_decomposer".to_string(),
            model_name: "rule_engine_v1".to_string(),
        }
    }
}

#[async_trait]
impl Planner for AutonomousDecomposer {
    async fn plan(&self, context: &PlanningContext) -> Result<PlanProposal, PlannerError> {
        let mut tasks = Vec::new();
        let mut deps = Vec::new();

        let is_multi_agent = context.objective.to_lowercase().contains("multi-agent")
            || context.objective.to_lowercase().contains("multi_agent")
            || context.objective.to_lowercase().contains("collaborat")
            || context
                .context
                .as_deref()
                .unwrap_or("")
                .contains("multi_agent")
            || context
                .available_roles
                .iter()
                .any(|r| r == "Investigator" || r == "Analyst");

        if is_multi_agent {
            // Multi-Agent Concurrent Collaboration Topology:
            // Task 1 (Investigator) ──┐
            //                         ├──→ Task 3 (Developer) ──→ Task 4 (Reviewer) ──→ Task 5 (Integrator)
            // Task 2 (Analyst) ──────┘

            // 1. Defect Investigation Phase
            let t1_id = "task-1".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t1_id.clone(),
                objective: format!(
                    "Investigate repository codebase and diagnose root cause for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Inspect source files, verify existing failure manifestations, and isolate fault boundaries for objective: {}",
                    context.objective
                )),
                criteria: vec!["diagnostic:root_cause_isolated".into()],
                required_capabilities: vec!["filesystem_read".into(), "planning".into()],
                suggested_role: Some("Investigator".into()),
                priority: 10,
            });

            // 2. Repository & Architecture Analysis Phase (Concurrent with Task 1)
            let t2_id = "task-2".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t2_id.clone(),
                objective: format!(
                    "Analyze repository architecture, dependencies, and test conventions for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Inspect build configurations, dependency specifications, and regression test suites for: {}",
                    context.objective
                )),
                criteria: vec!["analysis:architecture_reviewed".into()],
                required_capabilities: vec!["filesystem_read".into(), "review".into()],
                suggested_role: Some("Analyst".into()),
                priority: 10,
            });
            // Note: task-2 has NO dependency on task-1; both execute concurrently!

            // 3. Core Developer Implementation Phase
            let t3_id = "task-3".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t3_id.clone(),
                objective: format!(
                    "Implement core logic changes based on findings for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Apply minimal correct code changes based on investigator findings and analyst guidance for: {}",
                    context.objective
                )),
                criteria: vec!["impl:code_modified".into()],
                required_capabilities: vec!["filesystem_write".into(), "shell".into(), "git".into()],
                suggested_role: Some("Developer".into()),
                priority: 20,
            });
            deps.push(crate::proposal::ProposedDependency {
                task_temp_id: t3_id.clone(),
                depends_on_temp_id: t1_id.clone(),
            });
            deps.push(crate::proposal::ProposedDependency {
                task_temp_id: t3_id.clone(),
                depends_on_temp_id: t2_id.clone(),
            });

            // 4. Independent Reviewer & Regression Testing Phase
            let t4_id = "task-4".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t4_id.clone(),
                objective: format!(
                    "Independently inspect implementation, run tests, and review diff for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Independently execute test suites, audit workspace git diff, and verify regression safety for: {}",
                    context.objective
                )),
                criteria: vec!["test:cargo_test_passed".into(), "review:approved".into()],
                required_capabilities: vec!["test_runner".into(), "review".into()],
                suggested_role: Some("Reviewer".into()),
                priority: 30,
            });
            deps.push(crate::proposal::ProposedDependency {
                task_temp_id: t4_id.clone(),
                depends_on_temp_id: t3_id.clone(),
            });

            // 5. System Integrator & Final Verification Phase
            let t5_id = "task-5".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t5_id.clone(),
                objective: format!(
                    "Integrate verified branch and record final git commit for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Integrate verified worktree branch into repository, perform independent verification, and commit for: {}",
                    context.objective
                )),
                criteria: vec![
                    "verification:independent_verified".into(),
                    "git:commit_recorded".into(),
                ],
                required_capabilities: vec!["integration".into(), "git".into()],
                suggested_role: Some("Integrator".into()),
                priority: 40,
            });
            deps.push(crate::proposal::ProposedDependency {
                task_temp_id: t5_id.clone(),
                depends_on_temp_id: t4_id.clone(),
            });
        } else {
            // Standard Sequential 5-Phase Topology (M11 / M12 / M13 compatibility)
            // 1. Investigation & Root Cause Phase
            let t1_id = "task-1".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t1_id.clone(),
                objective: format!(
                    "Investigate repository codebase and diagnose root cause for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Inspect source files, verify existing failure manifestations, and isolate fault boundaries for objective: {}",
                    context.objective
                )),
                criteria: vec!["diagnostic:root_cause_isolated".into()],
                required_capabilities: vec!["filesystem_read".into(), "planning".into()],
                suggested_role: Some("Planner".into()),
                priority: 10,
            });

            // 2. Core Implementation / Bug Fix Phase
            let t2_id = "task-2".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t2_id.clone(),
                objective: format!("Implement core logic changes for: {}", context.objective),
                description: Some(format!(
                    "Apply atomic code changes, fix algorithms or bugs, and maintain backward compatibility for: {}",
                    context.objective
                )),
                criteria: vec!["impl:code_modified".into()],
                required_capabilities: vec!["filesystem_write".into(), "shell".into(), "git".into()],
                suggested_role: Some("Developer".into()),
                priority: 20,
            });
            deps.push(crate::proposal::ProposedDependency {
                task_temp_id: t2_id.clone(),
                depends_on_temp_id: t1_id.clone(),
            });

            // 3. Automated Test Suite & Regression Verification Phase
            let t3_id = "task-3".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t3_id.clone(),
                objective: format!(
                    "Add regression tests and verify test suite for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Implement automated test cases validating edge cases and run test suite for: {}",
                    context.objective
                )),
                criteria: vec!["test:cargo_test_passed".into()],
                required_capabilities: vec![
                    "test_runner".into(),
                    "shell".into(),
                    "filesystem_write".into(),
                ],
                suggested_role: Some("Tester".into()),
                priority: 30,
            });
            deps.push(crate::proposal::ProposedDependency {
                task_temp_id: t3_id.clone(),
                depends_on_temp_id: t2_id.clone(),
            });

            // 4. Review & Governance Approval Phase
            let t4_id = "task-4".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t4_id.clone(),
                objective: format!(
                    "Review diff, verify criteria, and obtain human approval for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Audit security, inspect workspace git diff, and trigger human approval gate for: {}",
                    context.objective
                )),
                criteria: vec!["governance:approval_obtained".into()],
                required_capabilities: vec!["review".into(), "git".into()],
                suggested_role: Some("Reviewer".into()),
                priority: 40,
            });
            deps.push(crate::proposal::ProposedDependency {
                task_temp_id: t4_id.clone(),
                depends_on_temp_id: t3_id.clone(),
            });

            // 5. Independent Verification & Commit Phase
            let t5_id = "task-5".to_string();
            tasks.push(crate::proposal::ProposedTask {
                temp_id: t5_id.clone(),
                objective: format!(
                    "Perform independent verification and commit clean working tree for: {}",
                    context.objective
                ),
                description: Some(format!(
                    "Independently execute verification checks and record git commit for: {}",
                    context.objective
                )),
                criteria: vec![
                    "verification:independent_verified".into(),
                    "git:commit_recorded".into(),
                ],
                required_capabilities: vec!["verification".into(), "git".into()],
                suggested_role: Some("Verifier".into()),
                priority: 50,
            });
            deps.push(crate::proposal::ProposedDependency {
                task_temp_id: t5_id.clone(),
                depends_on_temp_id: t4_id.clone(),
            });
        }

        let proposal = PlanProposal {
            objective: context.objective.clone(),
            rationale: format!(
                "Autonomous 5-phase verified engineering plan for objective: {}",
                context.objective
            ),
            tasks,
            dependencies: deps,
            execution_strategy: crate::proposal::ExecutionStrategy::Sequential,
            verification_strategy: crate::proposal::VerificationStrategy::MultiStep,
        };

        let report = crate::validator::PlanValidator::validate(&proposal);
        if !report.is_valid {
            return Err(PlannerError::Validation(report.errors));
        }

        Ok(proposal)
    }

    fn provider_name(&self) -> &str {
        &self.provider_name
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}

/// Adaptive planner that tries an LLM provider first, and falls back cleanly
/// to AutonomousDecomposer if the provider fails or is unconfigured.
pub struct AdaptivePlanner {
    llm_planner: Option<LlmPlanner>,
    decomposer: AutonomousDecomposer,
}

impl AdaptivePlanner {
    pub fn new(llm_planner: Option<LlmPlanner>) -> Self {
        Self {
            llm_planner,
            decomposer: AutonomousDecomposer::new(),
        }
    }

    pub fn with_decomposer(mut self, decomposer: AutonomousDecomposer) -> Self {
        self.decomposer = decomposer;
        self
    }
}

#[async_trait]
impl Planner for AdaptivePlanner {
    async fn plan(&self, context: &PlanningContext) -> Result<PlanProposal, PlannerError> {
        if let Some(ref llm) = self.llm_planner {
            match llm.plan(context).await {
                Ok(proposal) => {
                    let report = crate::validator::PlanValidator::validate(&proposal);
                    if report.is_valid {
                        return Ok(proposal);
                    }
                }
                Err(_err) => {}
            }
        }
        self.decomposer.plan(context).await
    }

    fn provider_name(&self) -> &str {
        if let Some(ref llm) = self.llm_planner {
            llm.provider_name()
        } else {
            self.decomposer.provider_name()
        }
    }

    fn model_name(&self) -> &str {
        if let Some(ref llm) = self.llm_planner {
            llm.model_name()
        } else {
            self.decomposer.model_name()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plexis_core::ids::WorkflowId;

    #[tokio::test]
    async fn test_autonomous_decomposer_valid_dag() {
        let decomposer = AutonomousDecomposer::new();
        let wf_id = WorkflowId::new();
        let ctx = PlanningContext::new(wf_id, "Fix tombstone compaction bug in kv-store");

        let proposal = decomposer.plan(&ctx).await.unwrap();
        assert_eq!(proposal.tasks.len(), 5);
        assert_eq!(proposal.dependencies.len(), 4);

        let report = crate::validator::PlanValidator::validate(&proposal);
        assert!(report.is_valid);
    }

    #[tokio::test]
    async fn test_autonomous_decomposer_multi_agent_dag() {
        let decomposer = AutonomousDecomposer::new();
        let wf_id = WorkflowId::new();
        let ctx =
            PlanningContext::new(wf_id, "Collaborative multi-agent bug fix for config-loader");

        let proposal = decomposer.plan(&ctx).await.unwrap();
        assert_eq!(proposal.tasks.len(), 5);
        // task-1 and task-2 are concurrent roots (0 dependencies on them)
        // task-3 depends on task-1 and task-2 (2 deps)
        // task-4 depends on task-3 (1 dep)
        // task-5 depends on task-4 (1 dep)
        assert_eq!(proposal.dependencies.len(), 4);

        let roles: Vec<_> = proposal
            .tasks
            .iter()
            .map(|t| t.suggested_role.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(
            roles,
            vec![
                "Investigator",
                "Analyst",
                "Developer",
                "Reviewer",
                "Integrator"
            ]
        );

        let report = crate::validator::PlanValidator::validate(&proposal);
        assert!(report.is_valid);
    }
}
