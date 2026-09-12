//! Planner abstraction and LLM/Scripted implementations.

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use plexis_core::ids::{TaskId, WorkflowId};
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
            parent_task_id: None,
            objective: objective.into(),
            context: None,
            available_capabilities: Vec::new(),
            available_roles: Vec::new(),
            failure_diagnostics: None,
        }
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
             Do NOT wrap the JSON in commentary or conversational text."
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
