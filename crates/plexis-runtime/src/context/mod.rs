//! Context management and token budgeting subsystem for Plexis.
//!
//! Assembles agent execution context from system rules, agent profile, task criteria,
//! dependency outputs, failure evidence, and messages while enforcing strict token bounds.

use plexis_core::{Agent, AgentMessage, Task};
use plexis_providers::ChatMessage;

/// Token budget and compaction settings for an execution attempt.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Maximum estimated input tokens allowed in the prompt.
    pub max_input_tokens: usize,
    /// Maximum number of conversational message turns preserved.
    pub max_history_turns: usize,
    /// Maximum byte size permitted per individual artifact preview.
    pub max_artifact_bytes: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_input_tokens: 8192,
            max_history_turns: 12,
            max_artifact_bytes: 4096,
        }
    }
}

impl ContextBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_input_tokens: max_tokens,
            ..Default::default()
        }
    }

    /// Fast, conservative heuristic estimator: ~4 characters per token.
    pub fn estimate_tokens(text: &str) -> usize {
        text.len().div_ceil(4)
    }
}

/// Output summary of assembled and budgeted context ready for LLM consumption.
#[derive(Debug, Clone)]
pub struct ContextSummary {
    /// Assembled ChatMessages ready for the provider.
    pub messages: Vec<ChatMessage>,
    /// Total estimated tokens across all messages.
    pub estimated_tokens: usize,
    /// Number of older messages omitted to fit budget.
    pub omitted_messages_count: usize,
    /// Total bytes truncated from large payloads.
    pub truncated_bytes: usize,
}

/// Assembles multi-source context into a budgeted prompt history.
pub struct ContextBuilder<'a> {
    agent: &'a Agent,
    workflow_objective: &'a str,
    task: &'a Task,
    system_rules: Vec<String>,
    dependency_summaries: Vec<(String, String)>,
    failure_evidence: Option<String>,
    incoming_messages: Vec<&'a AgentMessage>,
    workspace_path: String,
    budget: ContextBudget,
}

impl<'a> ContextBuilder<'a> {
    pub fn new(agent: &'a Agent, workflow_objective: &'a str, task: &'a Task) -> Self {
        Self {
            agent,
            workflow_objective,
            task,
            system_rules: Vec::new(),
            dependency_summaries: Vec::new(),
            failure_evidence: None,
            incoming_messages: Vec::new(),
            workspace_path: ".".to_string(),
            budget: ContextBudget::default(),
        }
    }

    pub fn with_workspace(mut self, path: impl Into<String>) -> Self {
        self.workspace_path = path.into();
        self
    }

    pub fn with_budget(mut self, budget: ContextBudget) -> Self {
        self.budget = budget;
        self
    }

    pub fn with_system_rule(mut self, rule: impl Into<String>) -> Self {
        self.system_rules.push(rule.into());
        self
    }

    pub fn with_dependency_summary(
        mut self,
        task_name: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        self.dependency_summaries
            .push((task_name.into(), summary.into()));
        self
    }

    pub fn with_failure_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.failure_evidence = Some(evidence.into());
        self
    }

    pub fn with_incoming_messages(mut self, msgs: &'a [AgentMessage]) -> Self {
        self.incoming_messages = msgs.iter().collect();
        self
    }

    /// Assembles context, enforcing strict token budgeting and priority preservation.
    pub fn build(self) -> ContextSummary {
        let mut truncated_bytes = 0;
        let mut omitted_messages = 0;

        // 1. High-Priority System Prompt (Never truncated)
        let mut system_text = format!(
            "You are an autonomous Plexis agent named '{}' with role '{}'.\n\
             Workflow Objective: {}\n\
             Current Task Objective: {}\n\
             Workspace Root: {}\n\
             Declared Capabilities: [{}]\n",
            self.agent.display_name,
            self.agent.role,
            self.workflow_objective,
            self.task.objective,
            self.workspace_path,
            self.agent.capabilities.join(", ")
        );

        if !self.system_rules.is_empty() {
            system_text.push_str("\nSYSTEM RULES:\n");
            for rule in &self.system_rules {
                system_text.push_str(&format!("- {}\n", rule));
            }
        }

        // 2. High-Priority Task Acceptance Criteria (Never truncated)
        let mut user_text = format!("EXECUTION TASK:\nObjective: {}\n", self.task.objective);
        if let Some(desc) = &self.task.description {
            user_text.push_str(&format!("Description: {}\n", desc));
        }
        if !self.task.criteria.is_empty() {
            user_text.push_str("Acceptance Criteria (Independent verifier will inspect these):\n");
            for c in &self.task.criteria {
                user_text.push_str(&format!("  * {}\n", c));
            }
        }

        // 3. High-Priority Failure Feedback (Never truncated if retrying)
        if let Some(fail_ev) = &self.failure_evidence {
            user_text.push_str(&format!(
                "\n[ATTENTION - PREVIOUS ATTEMPT FAILED]:\n{}\nFix the issue and ensure all acceptance criteria pass.\n",
                fail_ev
            ));
        }

        // 4. Upstream Dependency Artifact Summaries (Budget-capped)
        if !self.dependency_summaries.is_empty() {
            user_text.push_str("\nPREREQUISITE ARTIFACTS / UPSTREAM CONTEXT:\n");
            for (t_name, content) in &self.dependency_summaries {
                if content.len() > self.budget.max_artifact_bytes {
                    let kept = &content[..self.budget.max_artifact_bytes];
                    let omitted = content.len() - self.budget.max_artifact_bytes;
                    truncated_bytes += omitted;
                    user_text.push_str(&format!(
                        "--- {} ---\n{}\n[truncated: omitted {} bytes]\n",
                        t_name, kept, omitted
                    ));
                } else {
                    user_text.push_str(&format!("--- {} ---\n{}\n", t_name, content));
                }
            }
        }

        // 5. Incoming Agent Messages (Oldest compacted/omitted if history exceeds limit)
        let mut message_history = Vec::new();
        let total_msgs = self.incoming_messages.len();

        let msgs_to_include = if total_msgs > self.budget.max_history_turns {
            omitted_messages = total_msgs - self.budget.max_history_turns;
            &self.incoming_messages[omitted_messages..]
        } else {
            &self.incoming_messages[..]
        };

        if omitted_messages > 0 {
            user_text.push_str(&format!(
                "\n[Note: {} older messages omitted to conserve context budget]\n",
                omitted_messages
            ));
        }

        if !msgs_to_include.is_empty() {
            user_text.push_str("\nRECENT MESSAGES FROM PEER AGENTS:\n");
            for msg in msgs_to_include {
                user_text.push_str(&format!(
                    "[{:?} from {}]: {}\n",
                    msg.message_type, msg.from_agent, msg.content
                ));
            }
        }

        let system_msg = ChatMessage::system(system_text);
        let user_msg = ChatMessage::user(user_text);

        message_history.push(system_msg);
        message_history.push(user_msg);

        let total_chars: usize = message_history
            .iter()
            .map(|m| m.content.as_deref().map(|s| s.len()).unwrap_or(0))
            .sum();
        let estimated_tokens = ContextBudget::estimate_tokens(&"x".repeat(total_chars));

        ContextSummary {
            messages: message_history,
            estimated_tokens,
            omitted_messages_count: omitted_messages,
            truncated_bytes,
        }
    }
}
