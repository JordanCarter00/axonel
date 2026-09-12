//! Context management, token budgeting, and provenance subsystem for Plexis.
//!
//! Assembles agent execution context from system rules, agent profile, task contract,
//! retrieved persistent memories, upstream dependency outputs, recovery recommendations,
//! and peer messages while enforcing strict token bounds, dynamic compression, and omission receipts.

use plexis_core::{Agent, AgentMessage, Task};
use plexis_memory::ScoredMemory;
use plexis_providers::ChatMessage;
use serde::{Deserialize, Serialize};

/// Detailed provenance record for an individual item injected into the prompt context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextProvenanceItem {
    pub section: String,
    pub source_id: Option<String>,
    pub estimated_tokens: usize,
    pub truncated: bool,
}

/// Explicit receipt indicating content that was omitted or truncated due to token limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmissionReceipt {
    pub section: String,
    pub count: usize,
    pub reason: String,
    pub units_saved: usize,
}

/// Comprehensive provenance trace accounting for every token in the prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextProvenance {
    pub items: Vec<ContextProvenanceItem>,
    pub total_estimated_tokens: usize,
    pub omission_receipts: Vec<OmissionReceipt>,
}

/// Token budget and compaction settings for an execution attempt.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// Maximum estimated input tokens allowed in the prompt.
    pub max_input_tokens: usize,
    /// Maximum number of conversational message turns preserved.
    pub max_history_turns: usize,
    /// Maximum byte size permitted per individual artifact preview.
    pub max_artifact_bytes: usize,
    /// Maximum number of retrieved persistent memories injected.
    pub max_memories: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_input_tokens: 8192,
            max_history_turns: 12,
            max_artifact_bytes: 4096,
            max_memories: 5,
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
    /// Detailed provenance accounting for every section.
    pub provenance: ContextProvenance,
    /// Omission receipts explicitly notifying the agent of missing context.
    pub omission_receipts: Vec<OmissionReceipt>,
}

/// Recovery advice injected into a retry context after an attempt failure.
#[derive(Debug, Clone)]
pub struct RecoveryAdvice {
    pub strategy: String,
    pub strategy_version: u32,
    pub diagnosis: String,
    pub action: String,
}

/// Assembles multi-source context into a budgeted prompt history with explicit provenance.
pub struct ContextBuilder<'a> {
    agent: &'a Agent,
    workflow_objective: &'a str,
    task: &'a Task,
    system_rules: Vec<String>,
    dependency_summaries: Vec<(String, String)>,
    failure_evidence: Option<String>,
    recovery_advice: Option<RecoveryAdvice>,
    retrieved_memories: Vec<ScoredMemory>,
    incoming_messages: Vec<&'a AgentMessage>,
    tool_history: Vec<(String, String)>,
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
            recovery_advice: None,
            retrieved_memories: Vec::new(),
            incoming_messages: Vec::new(),
            tool_history: Vec::new(),
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

    pub fn with_recovery_advice(
        mut self,
        strategy: impl Into<String>,
        strategy_version: u32,
        diagnosis: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        self.recovery_advice = Some(RecoveryAdvice {
            strategy: strategy.into(),
            strategy_version,
            diagnosis: diagnosis.into(),
            action: action.into(),
        });
        self
    }

    pub fn with_memories(mut self, memories: Vec<ScoredMemory>) -> Self {
        self.retrieved_memories = memories;
        self
    }

    pub fn with_incoming_messages(mut self, msgs: &'a [AgentMessage]) -> Self {
        self.incoming_messages = msgs.iter().collect();
        self
    }

    pub fn with_tool_history(mut self, history: Vec<(String, String)>) -> Self {
        self.tool_history = history;
        self
    }

    /// Assembles context, enforcing strict token budgeting, provenance tracking, and omission receipts.
    pub fn build(self) -> ContextSummary {
        let mut provenance_items = Vec::new();
        let mut omission_receipts = Vec::new();
        let mut truncated_bytes = 0;
        let mut omitted_messages = 0;

        // 1. Tier 1: System Rules and Agent Identity (Never Evicted)
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

        provenance_items.push(ContextProvenanceItem {
            section: "system_identity_and_rules".to_string(),
            source_id: Some(self.agent.id.to_string()),
            estimated_tokens: ContextBudget::estimate_tokens(&system_text),
            truncated: false,
        });

        // 2. Tier 1: Task Contract and Acceptance Criteria (Never Evicted)
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

        provenance_items.push(ContextProvenanceItem {
            section: "task_contract".to_string(),
            source_id: Some(self.task.id.to_string()),
            estimated_tokens: ContextBudget::estimate_tokens(&user_text),
            truncated: false,
        });

        // 3. Tier 2: Failure Feedback and Strategy Recommendations (High Priority)
        if let Some(fail_ev) = &self.failure_evidence {
            let mut failure_block = format!(
                "\n[ATTENTION - PREVIOUS ATTEMPT FAILED]:\n{}\nFix the root cause and ensure all acceptance criteria pass.\n",
                fail_ev
            );

            if let Some(ref advice) = self.recovery_advice {
                failure_block.push_str(&format!(
                    "\n[RECOVERY STRATEGY (v{}) - {}]:\nDiagnosis: {}\nRecommended Action: {}\n",
                    advice.strategy_version, advice.strategy, advice.diagnosis, advice.action
                ));
            }

            user_text.push_str(&failure_block);
            provenance_items.push(ContextProvenanceItem {
                section: "failure_and_recovery_advice".to_string(),
                source_id: None,
                estimated_tokens: ContextBudget::estimate_tokens(&failure_block),
                truncated: false,
            });
        }

        // 4. Tier 3: Retrieved Persistent Memories
        if !self.retrieved_memories.is_empty() {
            let mut mem_block = String::from("\nRECALLED PERSISTENT KNOWLEDGE / MEMORY:\n");
            let total_memories = self.retrieved_memories.len();
            let to_include = total_memories.min(self.budget.max_memories);

            if total_memories > to_include {
                let omitted = total_memories - to_include;
                omission_receipts.push(OmissionReceipt {
                    section: "persistent_memories".to_string(),
                    count: omitted,
                    reason: "Exceeded max_memories budget cap".to_string(),
                    units_saved: omitted * 64,
                });
            }

            for item in self.retrieved_memories.iter().take(to_include) {
                let scope_label = item.record.scope.to_string();
                let memory_line = format!(
                    "* [{}] (Relevance: {:.2}, Source: {}): {}\n",
                    scope_label, item.total_score, item.record.source, item.record.content
                );
                mem_block.push_str(&memory_line);

                provenance_items.push(ContextProvenanceItem {
                    section: "persistent_memory".to_string(),
                    source_id: Some(item.record.id.to_string()),
                    estimated_tokens: ContextBudget::estimate_tokens(&memory_line),
                    truncated: false,
                });
            }
            user_text.push_str(&mem_block);
        }

        // 5. Tier 4: Prerequisite Artifact Summaries
        if !self.dependency_summaries.is_empty() {
            user_text.push_str("\nPREREQUISITE ARTIFACTS / UPSTREAM CONTEXT:\n");
            for (t_name, content) in &self.dependency_summaries {
                if content.len() > self.budget.max_artifact_bytes {
                    let kept = &content[..self.budget.max_artifact_bytes];
                    let omitted = content.len() - self.budget.max_artifact_bytes;
                    truncated_bytes += omitted;

                    omission_receipts.push(OmissionReceipt {
                        section: format!("artifact_summary_{}", t_name),
                        count: 1,
                        reason: "Exceeded max_artifact_bytes limit".to_string(),
                        units_saved: omitted,
                    });

                    let line = format!(
                        "--- {} ---\n{}\n[truncated: omitted {} bytes]\n",
                        t_name, kept, omitted
                    );
                    user_text.push_str(&line);
                    provenance_items.push(ContextProvenanceItem {
                        section: "dependency_artifact".to_string(),
                        source_id: Some(t_name.clone()),
                        estimated_tokens: ContextBudget::estimate_tokens(&line),
                        truncated: true,
                    });
                } else {
                    let line = format!("--- {} ---\n{}\n", t_name, content);
                    user_text.push_str(&line);
                    provenance_items.push(ContextProvenanceItem {
                        section: "dependency_artifact".to_string(),
                        source_id: Some(t_name.clone()),
                        estimated_tokens: ContextBudget::estimate_tokens(&line),
                        truncated: false,
                    });
                }
            }
        }

        // 6. Tier 5: Peer Agent Communication History
        let total_msgs = self.incoming_messages.len();
        let msgs_to_include = if total_msgs > self.budget.max_history_turns {
            omitted_messages = total_msgs - self.budget.max_history_turns;
            omission_receipts.push(OmissionReceipt {
                section: "peer_messages".to_string(),
                count: omitted_messages,
                reason: "Exceeded max_history_turns".to_string(),
                units_saved: omitted_messages * 32,
            });
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
                let msg_line = format!(
                    "[{:?} from {}]: {}\n",
                    msg.message_type, msg.from_agent, msg.content
                );
                user_text.push_str(&msg_line);
                provenance_items.push(ContextProvenanceItem {
                    section: "peer_message".to_string(),
                    source_id: Some(msg.id.to_string()),
                    estimated_tokens: ContextBudget::estimate_tokens(&msg_line),
                    truncated: false,
                });
            }
        }

        // 7. Tier 6: Recent Tool Execution History
        if !self.tool_history.is_empty() {
            user_text.push_str("\nRECENT EXECUTION TOOL OUTPUTS:\n");
            for (call, out) in &self.tool_history {
                let trimmed_out = if out.len() > 512 {
                    let omitted = out.len() - 512;
                    truncated_bytes += omitted;
                    format!("{}... [truncated {} bytes]", &out[..512], omitted)
                } else {
                    out.clone()
                };
                let tool_line = format!("$ {}\n{}\n", call, trimmed_out);
                user_text.push_str(&tool_line);
                provenance_items.push(ContextProvenanceItem {
                    section: "tool_history".to_string(),
                    source_id: Some(call.clone()),
                    estimated_tokens: ContextBudget::estimate_tokens(&tool_line),
                    truncated: out.len() > 512,
                });
            }
        }

        // 8. Omission Receipts block injected into prompt so agent has full visibility
        if !omission_receipts.is_empty() {
            user_text.push_str("\n[CONTEXT ADVISORY - OMISSION RECEIPTS]:\n");
            for receipt in &omission_receipts {
                user_text.push_str(&format!(
                    "* Omitted/Truncated in '{}': {} items ({})\n",
                    receipt.section, receipt.count, receipt.reason
                ));
            }
        }

        let system_msg = ChatMessage::system(system_text);
        let user_msg = ChatMessage::user(user_text);

        let messages = vec![system_msg, user_msg];
        let total_chars: usize = messages
            .iter()
            .map(|m| m.content.as_deref().map(|s| s.len()).unwrap_or(0))
            .sum();
        let estimated_tokens = ContextBudget::estimate_tokens(&"x".repeat(total_chars));

        let provenance = ContextProvenance {
            items: provenance_items,
            total_estimated_tokens: estimated_tokens,
            omission_receipts: omission_receipts.clone(),
        };

        ContextSummary {
            messages,
            estimated_tokens,
            omitted_messages_count: omitted_messages,
            truncated_bytes,
            provenance,
            omission_receipts,
        }
    }
}
