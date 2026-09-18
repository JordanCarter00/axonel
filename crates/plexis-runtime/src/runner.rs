use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use plexis_core::ids::{AgentId, TaskId};
use plexis_core::protocol::{ExecutionEvent, ExecutionEventType, ExecutionRequest};
use plexis_core::state::{AgentState, TaskState};
use plexis_core::{
    Agent, AgentMessage, Command, CommandType, Event, Execution, MessageType, Session,
    VerificationVerdict,
};
use plexis_providers::{ChatMessage, CompletionRequest, Provider, ToolDefinition};
use plexis_storage::traits::{
    AgentStore, ApprovalStore, CommandStore, EventStore, ExecutionStore, LeaseStore, MemoryStore,
    MessageStore, RecoveryStore, SessionStore, TaskStore, VerificationStore, WorkflowStore,
};
use plexis_tools::{Sandbox, ToolInvocationContext, ToolRegistry};
use tokio::sync::mpsc;

use crate::context::ContextBuilder;
use crate::error::RuntimeError;
use crate::governance::GovernanceManager;
use crate::recovery::RecoveryController;
use crate::verifier::WorkspaceVerifier;

pub type TerminalCallback = Arc<dyn Fn(&TaskId, &str, &str) + Send + Sync>;

/// Central runner executing an assigned task attempt against an agent and provider.
pub struct AgentRunner<
    S: WorkflowStore
        + TaskStore
        + AgentStore
        + SessionStore
        + ExecutionStore
        + CommandStore
        + EventStore
        + VerificationStore
        + MessageStore
        + ApprovalStore
        + MemoryStore
        + RecoveryStore
        + LeaseStore
        + 'static,
> {
    store: Arc<S>,
    providers: HashMap<String, Arc<dyn Provider>>,
    tool_registry: ToolRegistry,
    verifier: Arc<WorkspaceVerifier<S>>,
    recovery_controller: Option<Arc<RecoveryController<S>>>,
    terminal_callback: Option<TerminalCallback>,
    agent_backends: HashMap<String, Arc<dyn crate::backend::AgentBackend>>,
}

impl<
        S: WorkflowStore
            + TaskStore
            + AgentStore
            + SessionStore
            + ExecutionStore
            + CommandStore
            + EventStore
            + VerificationStore
            + MessageStore
            + ApprovalStore
            + MemoryStore
            + RecoveryStore
            + LeaseStore
            + 'static,
    > AgentRunner<S>
{
    pub fn new(
        store: Arc<S>,
        tool_registry: ToolRegistry,
        verifier: Arc<WorkspaceVerifier<S>>,
    ) -> Self {
        Self {
            store,
            providers: HashMap::new(),
            agent_backends: HashMap::new(),
            tool_registry,
            verifier,
            recovery_controller: None,
            terminal_callback: None,
        }
    }

    pub fn with_recovery_controller(mut self, controller: Arc<RecoveryController<S>>) -> Self {
        self.recovery_controller = Some(controller);
        self
    }

    pub fn with_terminal_callback(mut self, callback: TerminalCallback) -> Self {
        self.terminal_callback = Some(callback);
        self
    }

    pub fn register_provider(&mut self, provider: Arc<dyn Provider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn register_backend(&mut self, backend: Arc<dyn crate::backend::AgentBackend>) {
        self.agent_backends
            .insert(backend.id().to_string(), backend);
    }

    pub fn with_backend(mut self, backend: Arc<dyn crate::backend::AgentBackend>) -> Self {
        self.register_backend(backend);
        self
    }

    /// Executes an assigned command for task execution end-to-end.
    pub async fn execute_command(&self, command: &Command) -> Result<Execution, RuntimeError> {
        let (task_id, target_agent_id) = match (&command.command_type, &command.target) {
            (CommandType::ExecuteTask, plexis_core::CommandTarget::Agent(aid)) => {
                let tid_str = command
                    .payload
                    .get("task_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        RuntimeError::InvalidCommand("Missing 'task_id' in command payload".into())
                    })?;
                let tid: TaskId = tid_str
                    .parse()
                    .map_err(|e| RuntimeError::InvalidCommand(format!("Invalid task_id: {}", e)))?;
                (tid, *aid)
            }
            (CommandType::ExecuteTask, plexis_core::CommandTarget::Task(tid)) => {
                let aid_str = command
                    .payload
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        RuntimeError::InvalidCommand("Missing 'agent_id' in command payload".into())
                    })?;
                let aid: AgentId = aid_str.parse().map_err(|e| {
                    RuntimeError::InvalidCommand(format!("Invalid agent_id: {}", e))
                })?;
                (*tid, aid)
            }
            _ => {
                return Err(RuntimeError::InvalidCommand(
                    "Command is not an ExecuteTask command".into(),
                ))
            }
        };

        // 1. Load entities
        let mut task = self
            .store
            .get_task(&task_id)
            .await
            .map_err(RuntimeError::Storage)?
            .ok_or_else(|| RuntimeError::InvalidCommand(format!("Task {} not found", task_id)))?;

        let mut agent = self
            .store
            .get_agent(&target_agent_id)
            .await
            .map_err(RuntimeError::Storage)?
            .ok_or_else(|| {
                RuntimeError::InvalidCommand(format!("Agent {} not found", target_agent_id))
            })?;

        // 1.5. Lease fencing token validation
        let active_lease = self
            .store
            .get_lease_by_task(&task_id)
            .await
            .map_err(RuntimeError::Storage)?;

        let expected_lease_gen = command
            .payload
            .get("lease_generation")
            .and_then(|v| v.as_u64());

        if let Some(expected_gen) = expected_lease_gen {
            match &active_lease {
                Some(lease) => {
                    if lease.agent_id != target_agent_id {
                        return Err(RuntimeError::Lease(format!(
                            "Lease fencing error: lease for task {} is held by agent {}, not {}",
                            task_id, lease.agent_id, target_agent_id
                        )));
                    }
                    if lease.generation != expected_gen {
                        return Err(RuntimeError::Lease(format!(
                            "Stale lease generation fencing token {} for task {} (current generation: {})",
                            expected_gen, task_id, lease.generation
                        )));
                    }
                    if lease.is_expired(Utc::now()) {
                        return Err(RuntimeError::Lease(format!(
                            "Lease expired for task {} held by agent {}",
                            task_id, target_agent_id
                        )));
                    }
                }
                None => {
                    return Err(RuntimeError::Lease(format!(
                        "No active lease found for task {} requiring fencing token {}",
                        task_id, expected_gen
                    )));
                }
            }
        } else if let Some(lease) = &active_lease {
            if lease.is_expired(Utc::now()) {
                return Err(RuntimeError::Lease(format!(
                    "Lease expired for task {} held by agent {}",
                    task_id, target_agent_id
                )));
            }
            if lease.agent_id != target_agent_id {
                return Err(RuntimeError::Lease(format!(
                    "Lease conflict: task {} is leased to agent {}, not {}",
                    task_id, lease.agent_id, target_agent_id
                )));
            }
        }

        // 2. Resolve or create persistent session
        let session = self.get_or_create_session(&agent).await?;
        let working_dir = session
            .working_directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        // 3. Create and initialize execution attempt
        let attempt = task.attempts + 1;
        task.attempts = attempt;
        task.assigned_agent_id = Some(agent.id);
        match task.state {
            TaskState::Backlog => {
                let _ = task.transition_to(TaskState::Ready);
                let _ = task.transition_to(TaskState::Assigned);
                let _ = task.transition_to(TaskState::Running);
            }
            TaskState::Ready => {
                let _ = task.transition_to(TaskState::Assigned);
                let _ = task.transition_to(TaskState::Running);
            }
            TaskState::Assigned => {
                let _ = task.transition_to(TaskState::Running);
            }
            _ => {}
        }
        self.store
            .update_task(&task)
            .await
            .map_err(RuntimeError::Storage)?;

        let mut execution = Execution::new(task.id, agent.id, attempt).with_session(session.id);
        execution
            .mark_running()
            .map_err(|e| RuntimeError::InvalidCommand(e.to_string()))?;
        self.store
            .create_execution(&execution)
            .await
            .map_err(RuntimeError::Storage)?;

        // Update agent state to Busy
        agent.state = AgentState::Busy;
        agent.current_execution_id = Some(execution.id);
        self.store
            .update_agent(&agent)
            .await
            .map_err(RuntimeError::Storage)?;

        // 4. Emit execution_started and agent_started events
        let exec_started_evt = Event::new(
            "execution",
            execution.id.to_string(),
            "execution_started",
            serde_json::json!({
                "task_id": task.id.to_string(),
                "agent_id": agent.id.to_string(),
                "attempt": attempt,
                "session_id": session.id.to_string(),
            }),
        );
        self.store
            .append_event(&exec_started_evt)
            .await
            .map_err(RuntimeError::Storage)?;

        let agent_started_evt = Event::new(
            "agent",
            agent.id.to_string(),
            "agent_started",
            serde_json::json!({
                "execution_id": execution.id.to_string(),
                "task_id": task.id.to_string(),
            }),
        );
        self.store
            .append_event(&agent_started_evt)
            .await
            .map_err(RuntimeError::Storage)?;

        // 4.5. Check for external agent backend execution mode
        let workflow = self
            .store
            .get_workflow(&task.workflow_id)
            .await
            .ok()
            .flatten();
        let external_backend_id: Option<String> = task
            .metadata
            .get("backend")
            .or_else(|| agent.configuration.get("backend"))
            .or_else(|| workflow.as_ref().and_then(|w| w.metadata.get("backend")))
            .and_then(|v| v.as_str())
            .filter(|b| *b != "scripted" && *b != "none" && *b != "provider")
            .map(|s| s.to_string())
            .or_else(|| {
                if task.metadata.get("execution_mode").and_then(|v| v.as_str())
                    == Some("external_agent")
                    || agent
                        .configuration
                        .get("execution_mode")
                        .and_then(|v| v.as_str())
                        == Some("external_agent")
                    || workflow
                        .as_ref()
                        .and_then(|w| w.metadata.get("execution_mode"))
                        .and_then(|v| v.as_str())
                        == Some("external_agent")
                    || agent.provider_profile.provider == "external"
                    || agent.provider_profile.provider == "fake_agent"
                {
                    Some("fake_agent".to_string())
                } else {
                    None
                }
            });

        if let Some(backend_id) = external_backend_id {
            return self
                .execute_external_backend(
                    &backend_id,
                    task,
                    agent,
                    execution,
                    working_dir,
                    expected_lease_gen,
                    command,
                )
                .await;
        }

        // 5. Lookup provider

        let provider = self
            .providers
            .get(&agent.provider_profile.provider)
            .cloned()
            .or_else(|| self.providers.values().next().cloned())
            .ok_or_else(|| {
                RuntimeError::InvalidCommand(format!(
                    "No provider registered for agent provider '{}'",
                    agent.provider_profile.provider
                ))
            })?;

        // 6. Setup sandbox and tool definitions
        let sandbox = Arc::new(Sandbox::new(&working_dir));
        let mut tool_defs: Vec<ToolDefinition> = self
            .tool_registry
            .list_tools()
            .into_iter()
            .map(|t| ToolDefinition::new(t.name(), t.description(), t.schema()))
            .collect();

        tool_defs.push(ToolDefinition::new(
            "send_message",
            "Send a durable message to another agent in this workflow",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "to_agent": { "type": "string", "description": "Target agent ID" },
                    "message_type": {
                        "type": "string",
                        "enum": ["question", "request", "result", "handoff", "warning", "review", "rejection", "proposal", "artifact_reference"]
                    },
                    "content": { "type": "string", "description": "Text message content" },
                    "payload": { "type": "object", "description": "Optional structured payload" }
                },
                "required": ["to_agent", "message_type", "content"]
            }),
        ));

        tool_defs.push(ToolDefinition::new(
            "request_human_approval",
            "Request explicit human authorization before executing a sensitive action",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "Action requiring authorization" },
                    "reason": { "type": "string", "description": "Justification for requiring authorization" }
                },
                "required": ["description"]
            }),
        ));

        tool_defs.push(ToolDefinition::new(
            "save_memory",
            "Persist a valuable project constraint, architecture decision, or discovery to durable memory",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "Knowledge, constraint, or insight to persist" },
                    "scope": {
                        "type": "string",
                        "enum": ["project", "workflow", "task", "agent"],
                        "description": "Scope of memory visibility (default: project)"
                    },
                    "importance": {
                        "type": "number",
                        "description": "Importance score between 0.0 and 1.0 (default: 0.8)"
                    }
                },
                "required": ["content"]
            }),
        ));

        tool_defs.push(ToolDefinition::new(
            "search_memory",
            "Retrieve persistent memories matching a text query",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Text search query" },
                    "scope": { "type": "string", "description": "Optional scope filter (project, workflow, task)" }
                },
                "required": ["query"]
            }),
        ));

        // 7. Context assembly & token budgeting
        let incoming_messages = self
            .store
            .list_messages_for_agent(&agent.id)
            .await
            .unwrap_or_default();

        let wf = self
            .store
            .get_workflow(&task.workflow_id)
            .await
            .ok()
            .flatten();
        let wf_obj = wf
            .as_ref()
            .map(|w| w.objective.as_str())
            .unwrap_or(&task.objective);

        let mut context_builder = ContextBuilder::new(&agent, wf_obj, &task)
            .with_workspace(working_dir.display().to_string())
            .with_incoming_messages(&incoming_messages);

        // Inject persistent project and workflow memories
        let mut memories_to_inject = Vec::new();
        if let Ok(proj_mems) = self
            .store
            .list_memories_by_scope(plexis_core::MemoryScope::Project, None)
            .await
        {
            for m in proj_mems {
                memories_to_inject.push(crate::context::ScoredMemory {
                    record: m,
                    similarity: 0.9,
                    importance_score: 0.9,
                    recency_score: 1.0,
                    total_score: 0.92,
                });
            }
        }
        let wf_id_str = task.workflow_id.to_string();
        if let Ok(wf_mems) = self
            .store
            .list_memories_by_scope(plexis_core::MemoryScope::Workflow, Some(&wf_id_str))
            .await
        {
            for m in wf_mems {
                memories_to_inject.push(crate::context::ScoredMemory {
                    record: m,
                    similarity: 0.85,
                    importance_score: 0.85,
                    recency_score: 1.0,
                    total_score: 0.88,
                });
            }
        }
        if !memories_to_inject.is_empty() {
            context_builder = context_builder.with_memories(memories_to_inject);
        }

        // Inject mutated recovery advice if this is a retried task
        if let Some(advice) = task.metadata.get("recovery_advice") {
            let strategy = advice
                .get("strategy")
                .and_then(|v| v.as_str())
                .unwrap_or("tool_adaptation");
            let version = advice.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let failure = advice
                .get("failure")
                .and_then(|v| v.as_str())
                .unwrap_or("Previous attempt failed");
            let adjustment = advice
                .get("adjustment")
                .and_then(|v| v.as_str())
                .unwrap_or("Revise approach based on failure evidence");
            context_builder =
                context_builder.with_recovery_advice(strategy, version, failure, adjustment);
        }

        if task.attempts > 1 {
            if let Ok(verifs) = self.store.list_verifications_by_task(&task.id).await {
                if let Some(last_fail) = verifs
                    .iter()
                    .rev()
                    .find(|v| v.verdict == VerificationVerdict::Failed)
                {
                    if let Some(reason) = &last_fail.failure_reason {
                        context_builder = context_builder.with_failure_evidence(reason);
                    }
                }
            }
        }

        let context_summary = context_builder.build();

        let budget_evt = Event::new(
            "execution",
            execution.id.to_string(),
            "context_budget_applied",
            serde_json::json!({
                "estimated_tokens": context_summary.estimated_tokens,
                "omitted_messages": context_summary.omitted_messages_count,
                "truncated_bytes": context_summary.truncated_bytes,
            }),
        );
        let _ = self.store.append_event(&budget_evt).await;

        let mut messages = context_summary.messages;

        // 8. Agent interaction loop (bounded up to 10 turns)
        let max_turns = 10;
        let mut completed_cleanly = false;
        let mut last_error = None;

        for _turn in 0..max_turns {
            // Provider request started
            let req_started_evt = Event::new(
                "execution",
                execution.id.to_string(),
                "provider_request_started",
                serde_json::json!({
                    "provider": provider.id(),
                    "model": agent.provider_profile.model,
                    "turn": _turn,
                }),
            );
            self.store
                .append_event(&req_started_evt)
                .await
                .map_err(RuntimeError::Storage)?;

            let completion_req =
                CompletionRequest::new(agent.provider_profile.model.clone(), messages.clone())
                    .with_tools(tool_defs.clone());

            let completion_res = provider.complete(&completion_req).await;

            match completion_res {
                Err(err) => {
                    let err_msg = err.to_string();
                    let req_completed_evt = Event::new(
                        "execution",
                        execution.id.to_string(),
                        "provider_request_completed",
                        serde_json::json!({
                            "success": false,
                            "error": err_msg,
                        }),
                    );
                    self.store
                        .append_event(&req_completed_evt)
                        .await
                        .map_err(RuntimeError::Storage)?;
                    last_error = Some(err_msg);
                    break;
                }
                Ok(response) => {
                    // Provider request completed
                    let req_completed_evt = Event::new(
                        "execution",
                        execution.id.to_string(),
                        "provider_request_completed",
                        serde_json::json!({
                            "success": true,
                            "finish_reason": response.finish_reason,
                            "tokens": response.usage,
                        }),
                    );
                    self.store
                        .append_event(&req_completed_evt)
                        .await
                        .map_err(RuntimeError::Storage)?;

                    let resp_msg = response.message;

                    // Emit message_sent event if text was generated
                    if let Some(text) = &resp_msg.content {
                        let msg_evt = Event::new(
                            "execution",
                            execution.id.to_string(),
                            "message_sent",
                            serde_json::json!({
                                "role": "assistant",
                                "content": text,
                            }),
                        );
                        self.store
                            .append_event(&msg_evt)
                            .await
                            .map_err(RuntimeError::Storage)?;
                    }

                    // Process tool calls if requested
                    if let Some(tool_calls) = &resp_msg.tool_calls {
                        messages.push(resp_msg.clone());

                        for call in tool_calls {
                            let tool_args: serde_json::Value = serde_json::from_str(
                                &call.arguments,
                            )
                            .unwrap_or_else(|_| serde_json::json!({ "raw": call.arguments }));

                            let tool_started_evt = Event::new(
                                "execution",
                                execution.id.to_string(),
                                "tool_started",
                                serde_json::json!({
                                    "tool": call.name,
                                    "tool_call_id": call.id,
                                    "arguments": tool_args,
                                }),
                            );
                            self.store
                                .append_event(&tool_started_evt)
                                .await
                                .map_err(RuntimeError::Storage)?;

                            // Intercept first-class messaging tool
                            if call.name == "send_message" {
                                let to_agent_str = tool_args
                                    .get("to_agent")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                let msg_type_str = tool_args
                                    .get("message_type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("result");
                                let content = tool_args
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                let payload = tool_args
                                    .get("payload")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::Null);

                                let to_id_res: Result<AgentId, _> = to_agent_str.parse();
                                match to_id_res {
                                    Ok(to_id) => {
                                        let msg_type: MessageType =
                                            serde_json::from_str(&format!("\"{}\"", msg_type_str))
                                                .unwrap_or(MessageType::Result);
                                        let msg = AgentMessage::new(
                                            agent.id,
                                            to_id,
                                            task.workflow_id,
                                            msg_type,
                                            content,
                                        )
                                        .with_task(task.id)
                                        .with_payload(payload);

                                        let _ = self.store.send_message(&msg).await;

                                        let msg_evt = Event::new(
                                            "execution",
                                            execution.id.to_string(),
                                            "message_sent",
                                            serde_json::json!({
                                                "message_id": msg.id.to_string(),
                                                "to_agent": to_id.to_string(),
                                                "message_type": msg_type_str,
                                            }),
                                        );
                                        let _ = self.store.append_event(&msg_evt).await;

                                        let tool_comp = Event::new(
                                            "execution",
                                            execution.id.to_string(),
                                            "tool_completed",
                                            serde_json::json!({
                                                "tool": "send_message",
                                                "tool_call_id": call.id,
                                                "status": "delivered",
                                            }),
                                        );
                                        let _ = self.store.append_event(&tool_comp).await;

                                        messages.push(ChatMessage::tool_response(
                                            call.id.clone(),
                                            serde_json::json!({
                                                "status": "delivered",
                                                "message_id": msg.id.to_string(),
                                            })
                                            .to_string(),
                                        ));
                                    }
                                    Err(err) => {
                                        messages.push(ChatMessage::tool_response(
                                            call.id.clone(),
                                            serde_json::json!({
                                                "error": format!("Invalid to_agent ID: {}", err)
                                            })
                                            .to_string(),
                                        ));
                                    }
                                }
                                continue;
                            }

                            // Intercept human approval gate tool
                            if call.name == "request_human_approval" {
                                let desc = tool_args
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Action requiring human authorization");
                                let reason = tool_args
                                    .get("reason")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());

                                let gov = GovernanceManager::new(self.store.clone());
                                let appr_res = gov
                                    .request_approval(
                                        task.id,
                                        task.workflow_id,
                                        Some(agent.id),
                                        desc,
                                        reason,
                                    )
                                    .await;

                                match appr_res {
                                    Ok(appr) => {
                                        task.state = TaskState::NeedsHuman;
                                        let tool_comp = Event::new(
                                            "execution",
                                            execution.id.to_string(),
                                            "tool_completed",
                                            serde_json::json!({
                                                "tool": "request_human_approval",
                                                "tool_call_id": call.id,
                                                "status": "approval_requested",
                                                "approval_id": appr.id.to_string(),
                                            }),
                                        );
                                        let _ = self.store.append_event(&tool_comp).await;

                                        messages.push(ChatMessage::tool_response(
                                            call.id.clone(),
                                            serde_json::json!({
                                                "status": "approval_requested",
                                                "approval_id": appr.id.to_string(),
                                                "state": "needs_human",
                                            })
                                            .to_string(),
                                        ));

                                        // Stop further execution turns until approved
                                        completed_cleanly = true;
                                        break;
                                    }
                                    Err(e) => {
                                        messages.push(ChatMessage::tool_response(
                                            call.id.clone(),
                                            serde_json::json!({ "error": e.to_string() })
                                                .to_string(),
                                        ));
                                    }
                                }
                                continue;
                            }

                            // Intercept save_memory tool
                            if call.name == "save_memory" {
                                let content = tool_args
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                let scope_str = tool_args
                                    .get("scope")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("project");
                                let importance = tool_args
                                    .get("importance")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.8)
                                    as f32;

                                if scope_str == "system" {
                                    let tool_err_str = "Access Denied: Agents are not authorized to write to System memory scope";
                                    let tool_failed_evt = Event::new(
                                        "execution",
                                        execution.id.to_string(),
                                        "tool_failed",
                                        serde_json::json!({
                                            "tool": "save_memory",
                                            "tool_call_id": call.id,
                                            "error": tool_err_str,
                                        }),
                                    );
                                    let _ = self.store.append_event(&tool_failed_evt).await;
                                    messages.push(ChatMessage::tool_response(
                                        call.id.clone(),
                                        format!("Error: {}", tool_err_str),
                                    ));
                                    continue;
                                }

                                let scope = match scope_str {
                                    "workflow" => plexis_core::MemoryScope::Workflow,
                                    "task" => plexis_core::MemoryScope::Task,
                                    "agent" => plexis_core::MemoryScope::Agent,
                                    _ => plexis_core::MemoryScope::Project,
                                };

                                let mut record = plexis_core::MemoryRecord::new(
                                    scope,
                                    format!("agent_{}", agent.id),
                                    content,
                                );
                                record.importance = importance;
                                record.scope_id = match scope {
                                    plexis_core::MemoryScope::Workflow => {
                                        Some(task.workflow_id.to_string())
                                    }
                                    plexis_core::MemoryScope::Task => Some(task.id.to_string()),
                                    _ => None,
                                };
                                record.provenance.originating_workflow_id = Some(task.workflow_id);
                                record.provenance.originating_task_id = Some(task.id);
                                record.provenance.originating_agent_id = Some(agent.id);

                                match self.store.save_memory(&record).await {
                                    Ok(_) => {
                                        let mem_evt = Event::new(
                                            "execution",
                                            execution.id.to_string(),
                                            "memory_saved",
                                            serde_json::json!({
                                                "memory_id": record.id.to_string(),
                                                "scope": scope_str,
                                                "importance": importance,
                                            }),
                                        );
                                        let _ = self.store.append_event(&mem_evt).await;

                                        let tool_comp = Event::new(
                                            "execution",
                                            execution.id.to_string(),
                                            "tool_completed",
                                            serde_json::json!({
                                                "tool": "save_memory",
                                                "tool_call_id": call.id,
                                                "status": "saved",
                                                "memory_id": record.id.to_string(),
                                            }),
                                        );
                                        let _ = self.store.append_event(&tool_comp).await;

                                        messages.push(ChatMessage::tool_response(
                                            call.id.clone(),
                                            serde_json::json!({
                                                "status": "saved",
                                                "memory_id": record.id.to_string(),
                                            })
                                            .to_string(),
                                        ));
                                    }
                                    Err(e) => {
                                        messages.push(ChatMessage::tool_response(
                                            call.id.clone(),
                                            serde_json::json!({ "error": e.to_string() })
                                                .to_string(),
                                        ));
                                    }
                                }
                                continue;
                            }

                            // Intercept search_memory tool
                            if call.name == "search_memory" {
                                let query = tool_args
                                    .get("query")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                let scope_str = tool_args.get("scope").and_then(|v| v.as_str());

                                let records = if let Some(sc) = scope_str {
                                    let scope = match sc {
                                        "system" => plexis_core::MemoryScope::System,
                                        "workflow" => plexis_core::MemoryScope::Workflow,
                                        "task" => plexis_core::MemoryScope::Task,
                                        _ => plexis_core::MemoryScope::Project,
                                    };
                                    self.store
                                        .list_memories_by_scope(scope, None)
                                        .await
                                        .unwrap_or_default()
                                } else {
                                    let wf_str = task.workflow_id.to_string();
                                    self.store
                                        .list_memories_by_scope(
                                            plexis_core::MemoryScope::Workflow,
                                            Some(&wf_str),
                                        )
                                        .await
                                        .unwrap_or_default()
                                };

                                let q_lower = query.to_lowercase();
                                let matched: Vec<_> = records
                                    .into_iter()
                                    .filter(|r| {
                                        r.content.to_lowercase().contains(&q_lower)
                                            || query.is_empty()
                                    })
                                    .map(|r| {
                                        serde_json::json!({
                                            "id": r.id.to_string(),
                                            "scope": format!("{:?}", r.scope),
                                            "content": r.content,
                                            "importance": r.importance,
                                        })
                                    })
                                    .collect();

                                let tool_comp = Event::new(
                                    "execution",
                                    execution.id.to_string(),
                                    "tool_completed",
                                    serde_json::json!({
                                        "tool": "search_memory",
                                        "tool_call_id": call.id,
                                        "matches_count": matched.len(),
                                    }),
                                );
                                let _ = self.store.append_event(&tool_comp).await;

                                messages.push(ChatMessage::tool_response(
                                    call.id.clone(),
                                    serde_json::json!({ "results": matched }).to_string(),
                                ));
                                continue;
                            }

                            let tool_ctx = ToolInvocationContext::new(
                                agent.id,
                                execution.id,
                                task.id,
                                tool_args,
                                sandbox.clone(),
                                working_dir.clone(),
                            );

                            let (_record, tool_res) =
                                self.tool_registry.invoke(&call.name, &tool_ctx).await;

                            match tool_res {
                                Ok(mut output) => {
                                    // Automatically redact secrets from tool output before persistence & model return
                                    plexis_tools::SecretRedactor::redact_value_all(
                                        &mut output.data,
                                        &[],
                                    );

                                    if let Some(ref cb) = self.terminal_callback {
                                        if let Some(stdout) =
                                            output.data.get("stdout").and_then(|v| v.as_str())
                                        {
                                            cb(&task.id, "stdout", stdout);
                                        }
                                        if let Some(stderr) =
                                            output.data.get("stderr").and_then(|v| v.as_str())
                                        {
                                            cb(&task.id, "stderr", stderr);
                                        }
                                    }

                                    let tool_completed_evt = Event::new(
                                        "execution",
                                        execution.id.to_string(),
                                        "tool_completed",
                                        serde_json::json!({
                                            "tool": call.name,
                                            "tool_call_id": call.id,
                                            "output": output.data,
                                            "exit_code": output.exit_code,
                                        }),
                                    );
                                    self.store
                                        .append_event(&tool_completed_evt)
                                        .await
                                        .map_err(RuntimeError::Storage)?;

                                    let output_content =
                                        serde_json::to_string(&output.data).unwrap_or_default();
                                    messages.push(ChatMessage::tool_response(
                                        call.id.clone(),
                                        output_content,
                                    ));
                                }
                                Err(tool_err) => {
                                    let tool_err_str =
                                        plexis_tools::SecretRedactor::redact_patterns(
                                            &tool_err.to_string(),
                                        );
                                    let tool_failed_evt = Event::new(
                                        "execution",
                                        execution.id.to_string(),
                                        "tool_failed",
                                        serde_json::json!({
                                            "tool": call.name,
                                            "tool_call_id": call.id,
                                            "error": tool_err_str,
                                        }),
                                    );
                                    self.store
                                        .append_event(&tool_failed_evt)
                                        .await
                                        .map_err(RuntimeError::Storage)?;

                                    messages.push(ChatMessage::tool_response(
                                        call.id.clone(),
                                        format!("Error: {}", tool_err_str),
                                    ));
                                }
                            }
                        }
                    } else {
                        // Model did not invoke any tools; task turn cycle complete
                        completed_cleanly = true;
                        break;
                    }
                }
            }
        }

        // 9. Close execution lifecycle
        if let Some(expected_gen) = expected_lease_gen {
            let active_lease = self
                .store
                .get_lease_by_task(&task_id)
                .await
                .map_err(RuntimeError::Storage)?;
            match active_lease {
                Some(ref lease)
                    if lease.generation == expected_gen
                        && !lease.is_expired(Utc::now())
                        && lease.agent_id == target_agent_id => {}
                _ => {
                    return Err(RuntimeError::Lease(format!(
                        "Lease lost or expired for task {} before mutation commit",
                        task_id
                    )));
                }
            }
        }

        if completed_cleanly {
            if task.state == TaskState::NeedsHuman {
                agent.state = AgentState::Idle;
                agent.current_execution_id = None;
                self.store
                    .update_agent(&agent)
                    .await
                    .map_err(RuntimeError::Storage)?;
                task.assigned_agent_id = None;
                self.store
                    .update_task(&task)
                    .await
                    .map_err(RuntimeError::Storage)?;
                return Ok(execution);
            }

            execution
                .mark_completed()
                .map_err(|e| RuntimeError::InvalidCommand(e.to_string()))?;
            self.store
                .update_execution(&execution)
                .await
                .map_err(RuntimeError::Storage)?;

            let exec_completed_evt = Event::new(
                "execution",
                execution.id.to_string(),
                "execution_completed",
                serde_json::json!({
                    "task_id": task.id.to_string(),
                    "status": "completed",
                }),
            );
            self.store
                .append_event(&exec_completed_evt)
                .await
                .map_err(RuntimeError::Storage)?;

            // 10. Run Independent Verification!
            let verif = self
                .verifier
                .verify_and_record(&mut task, &execution, &working_dir)
                .await
                .map_err(RuntimeError::Storage)?;

            if verif.verdict == VerificationVerdict::Failed {
                if let Some(rc) = &self.recovery_controller {
                    let fail_reason = verif
                        .failure_reason
                        .as_deref()
                        .unwrap_or("Verification criteria failed");
                    if let Ok((action, _rec)) = rc
                        .diagnose_and_recover(
                            &task,
                            &task.workflow_id,
                            Some(&execution.id),
                            task.attempts,
                            fail_reason,
                        )
                        .await
                    {
                        self.apply_recovery_action(&mut task, action, fail_reason)
                            .await;
                    }
                }
            }
        } else {
            let reason = last_error
                .unwrap_or_else(|| "Turn limit exceeded or execution aborted".to_string());
            execution
                .mark_failed(&reason)
                .map_err(|e| RuntimeError::InvalidCommand(e.to_string()))?;
            self.store
                .update_execution(&execution)
                .await
                .map_err(RuntimeError::Storage)?;

            let _ = task.transition_to(TaskState::Failed);
            self.store
                .update_task(&task)
                .await
                .map_err(RuntimeError::Storage)?;

            let exec_failed_evt = Event::new(
                "execution",
                execution.id.to_string(),
                "execution_failed",
                serde_json::json!({
                    "task_id": task.id.to_string(),
                    "reason": reason,
                }),
            );
            self.store
                .append_event(&exec_failed_evt)
                .await
                .map_err(RuntimeError::Storage)?;

            if let Some(rc) = &self.recovery_controller {
                if let Ok((action, _rec)) = rc
                    .diagnose_and_recover(
                        &task,
                        &task.workflow_id,
                        Some(&execution.id),
                        task.attempts,
                        &reason,
                    )
                    .await
                {
                    self.apply_recovery_action(&mut task, action, &reason).await;
                }
            }
        }

        // Return agent to Idle
        agent.state = AgentState::Idle;
        agent.current_execution_id = None;
        self.store
            .update_agent(&agent)
            .await
            .map_err(RuntimeError::Storage)?;

        // Update command state
        let mut updated_cmd = command.clone();
        if completed_cleanly {
            updated_cmd.mark_completed();
        } else {
            updated_cmd.mark_failed();
        }
        self.store
            .update_command(&updated_cmd)
            .await
            .map_err(RuntimeError::Storage)?;

        Ok(execution)
    }

    async fn get_or_create_session(&self, agent: &Agent) -> Result<Session, RuntimeError> {
        let sessions = self
            .store
            .list_sessions_by_agent(&agent.id)
            .await
            .map_err(RuntimeError::Storage)?;

        for s in sessions {
            if s.is_active() {
                return Ok(s);
            }
        }

        // Create new active session
        let session = Session::new(agent.id);
        self.store
            .create_session(&session)
            .await
            .map_err(RuntimeError::Storage)?;
        Ok(session)
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_external_backend(
        &self,
        backend_id: &str,
        mut task: plexis_core::Task,
        mut agent: Agent,
        mut execution: Execution,
        working_dir: PathBuf,
        expected_lease_gen: Option<u64>,
        command: &Command,
    ) -> Result<Execution, RuntimeError> {
        let backend = self
            .agent_backends
            .get(backend_id)
            .cloned()
            .or_else(|| {
                if backend_id == "fake_agent" {
                    Some(
                        Arc::new(crate::backend::FakeAgentBackend::with_default_host())
                            as Arc<dyn crate::backend::AgentBackend>,
                    )
                } else if backend_id == "gemini_cli" {
                    Some(Arc::new(crate::backend::GeminiCliBackend::default())
                        as Arc<dyn crate::backend::AgentBackend>)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                RuntimeError::InvalidCommand(format!(
                    "External agent backend '{}' not registered in runner",
                    backend_id
                ))
            })?;

        let workflow = self
            .store
            .get_workflow(&task.workflow_id)
            .await
            .ok()
            .flatten();

        let is_integrator = agent.role.eq_ignore_ascii_case("integrator");
        let worktree_isolation = task
            .metadata
            .get("worktree_isolation")
            .and_then(|v| v.as_bool())
            .or_else(|| {
                workflow.as_ref().and_then(|w| {
                    w.metadata
                        .get("worktree_isolation")
                        .and_then(|v| v.as_bool())
                })
            })
            .unwrap_or(false);

        let effective_worktree_isolation = worktree_isolation && !is_integrator;
        let target_dir = if effective_worktree_isolation {
            let role_clean = agent.role.to_lowercase().replace([' ', '/', '\\'], "_");
            let branch_name = format!("agent/{}-{}", role_clean, task.id);
            let wt_manager = crate::worktree::WorktreeManager::new(&working_dir);
            match wt_manager.create_worktree(&branch_name, None) {
                Ok(p) => {
                    task.metadata["worktree_path"] = serde_json::json!(p.display().to_string());
                    task.metadata["worktree_branch"] = serde_json::json!(branch_name);
                    let _ = self.store.update_task(&task).await;
                    p
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to create worktree: {}, falling back to working_dir",
                        e
                    );
                    working_dir.clone()
                }
            }
        } else {
            working_dir.clone()
        };

        let timeout_secs = task
            .metadata
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);
        let failure_mode = task
            .metadata
            .get("failure_mode")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let delay_ms = task.metadata.get("delay_ms").and_then(|v| v.as_u64());

        let mut request = ExecutionRequest::new(
            execution.id,
            agent.id,
            &agent.role,
            &task.objective,
            target_dir.clone(),
        )
        .with_timeout_secs(timeout_secs);

        if let Some(desc) = &task.description {
            request = request.with_description(desc);
        }
        if let Some(fm) = failure_mode {
            request = request.with_failure_mode(fm);
        }
        if let Some(d) = delay_ms {
            request = request.with_delay_ms(d);
        }
        if let Some(policy) = task
            .metadata
            .get("execution_policy")
            .and_then(|v| v.as_str())
            .or_else(|| {
                agent
                    .configuration
                    .get("execution_policy")
                    .and_then(|v| v.as_str())
            })
        {
            request = request.with_execution_policy(policy);
        }
        if let Some(model) = task
            .metadata
            .get("model")
            .and_then(|v| v.as_str())
            .or_else(|| agent.configuration.get("model").and_then(|v| v.as_str()))
        {
            request = request.with_model(model);
        }

        // Ingest prior workflow messages into objective
        if let Ok(messages) = self
            .store
            .list_messages_by_workflow(&task.workflow_id)
            .await
        {
            if !messages.is_empty() {
                let mut msg_summary =
                    String::from("\n\n### Prior Inter-Agent Collaboration & Findings:\n");
                for msg in messages {
                    let sender_role = msg
                        .payload
                        .get("role")
                        .and_then(|r| r.as_str())
                        .unwrap_or("Agent");
                    msg_summary.push_str(&format!(
                        "- [{:?}] {}: {}\n",
                        msg.message_type, sender_role, msg.content
                    ));
                }
                request.objective.push_str(&msg_summary);
            }
        }

        if let Some(src_branch) = task
            .metadata
            .get("integrate_branch")
            .and_then(|v| v.as_str())
        {
            let integration_note = format!(
                "\n\n### Integration Objective:\nMerge verified changes from branch '{}' into target branch, run tests to ensure regression-free status, and commit.",
                src_branch
            );
            request.objective.push_str(&integration_note);
        }

        request = request.with_metadata(serde_json::json!({
            "workflow_id": task.workflow_id.to_string(),
            "task_id": task.id.to_string(),
            "backend": backend_id,
        }));

        let (event_tx, mut event_rx) = mpsc::channel::<ExecutionEvent>(100);
        let terminal_cb = self.terminal_callback.clone();
        let task_id = task.id;
        let store = self.store.clone();
        let exec_id_str = execution.id.to_string();

        let event_forwarder = tokio::spawn(async move {
            while let Some(ev) = event_rx.recv().await {
                match &ev.event {
                    ExecutionEventType::Stdout { text } => {
                        if let Some(ref cb) = terminal_cb {
                            cb(&task_id, "stdout", text);
                        }
                    }
                    ExecutionEventType::Stderr { text } => {
                        if let Some(ref cb) = terminal_cb {
                            cb(&task_id, "stderr", text);
                        }
                    }
                    ExecutionEventType::ToolAction {
                        tool,
                        action,
                        details,
                    } => {
                        let tool_evt = Event::new(
                            "execution",
                            exec_id_str.clone(),
                            "tool_action",
                            serde_json::json!({
                                "tool": tool,
                                "action": action,
                                "details": details,
                            }),
                        );
                        let _ = store.append_event(&tool_evt).await;
                    }
                    ExecutionEventType::Progress {
                        percentage,
                        message,
                    } => {
                        let prog_evt = Event::new(
                            "execution",
                            exec_id_str.clone(),
                            "progress",
                            serde_json::json!({
                                "percentage": percentage,
                                "message": message,
                            }),
                        );
                        let _ = store.append_event(&prog_evt).await;
                    }
                    ExecutionEventType::Warning { message } => {
                        let warn_evt = Event::new(
                            "execution",
                            exec_id_str.clone(),
                            "warning",
                            serde_json::json!({ "message": message }),
                        );
                        let _ = store.append_event(&warn_evt).await;
                    }
                    _ => {}
                }
            }
        });

        let exec_outcome = backend.execute(&request, Some(event_tx)).await;
        let _ = event_forwarder.await;

        // Verify lease fencing token
        if let Some(expected_gen) = expected_lease_gen {
            let active_lease = self
                .store
                .get_lease_by_task(&task.id)
                .await
                .map_err(RuntimeError::Storage)?;
            match active_lease {
                Some(ref lease)
                    if lease.generation == expected_gen
                        && !lease.is_expired(Utc::now())
                        && lease.agent_id == agent.id => {}
                _ => {
                    return Err(RuntimeError::Lease(format!(
                        "Lease lost or expired for task {} during external agent execution",
                        task.id
                    )));
                }
            }
        }

        let mut success = false;
        match exec_outcome {
            Ok(result) => {
                execution.metadata["backend"] = serde_json::json!(backend.id());
                execution.metadata["exit_code"] = serde_json::json!(result.exit_code);
                execution.metadata["summary"] = serde_json::json!(result.summary);
                execution.metadata["changed_files"] = serde_json::json!(result.changed_files);
                task.metadata["backend"] = serde_json::json!(backend.id());
                task.metadata["changed_files"] = serde_json::json!(result.changed_files);
                if let Some(ref sha) = result.commit_sha {
                    execution.metadata["commit_sha"] = serde_json::json!(sha);
                    task.metadata["commit_sha"] = serde_json::json!(sha);
                }

                if result.success && result.exit_code == 0 {
                    success = true;
                    execution
                        .mark_completed()
                        .map_err(|e| RuntimeError::InvalidCommand(e.to_string()))?;
                    self.store
                        .update_execution(&execution)
                        .await
                        .map_err(RuntimeError::Storage)?;

                    let comp_evt = Event::new(
                        "execution",
                        execution.id.to_string(),
                        "execution_completed",
                        serde_json::json!({
                            "task_id": task.id.to_string(),
                            "backend": backend.id(),
                            "commit_sha": result.commit_sha,
                            "status": "completed",
                        }),
                    );
                    self.store
                        .append_event(&comp_evt)
                        .await
                        .map_err(RuntimeError::Storage)?;

                    // Record durable AgentMessage for inter-agent collaboration
                    let msg_type = match agent.role.to_lowercase().as_str() {
                        r if r.contains("investigat") => MessageType::Result,
                        r if r.contains("analyst") => MessageType::Result,
                        r if r.contains("developer") => MessageType::Handoff,
                        r if r.contains("reviewer") => MessageType::Review,
                        r if r.contains("integrat") => MessageType::Result,
                        _ => MessageType::Result,
                    };

                    let msg_content = if !result.summary.trim().is_empty() {
                        result.summary.clone()
                    } else {
                        format!(
                            "Task '{}' completed by {} agent",
                            task.objective, agent.role
                        )
                    };

                    let mut payload = serde_json::json!({
                        "role": agent.role,
                        "task_id": task.id.to_string(),
                        "backend": backend.id(),
                        "changed_files": result.changed_files,
                        "commit_sha": result.commit_sha,
                    });
                    if let Some(branch) = task
                        .metadata
                        .get("worktree_branch")
                        .and_then(|v| v.as_str())
                    {
                        payload["branch"] = serde_json::json!(branch);
                    }

                    let agent_msg = AgentMessage::new(
                        agent.id,
                        agent.id,
                        task.workflow_id,
                        msg_type,
                        msg_content.clone(),
                    )
                    .with_task(task.id)
                    .with_payload(payload);

                    let _ = self.store.send_message(&agent_msg).await;

                    let msg_evt = Event::new(
                        "execution",
                        execution.id.to_string(),
                        "message_sent",
                        serde_json::json!({
                            "message_id": agent_msg.id.to_string(),
                            "from_agent": agent.id.to_string(),
                            "role": agent.role,
                            "message_type": format!("{:?}", msg_type),
                            "content": msg_content,
                        }),
                    );
                    let _ = self.store.append_event(&msg_evt).await;

                    // Handle branch integration if configured
                    if let Some(src_branch) = task
                        .metadata
                        .get("integrate_branch")
                        .and_then(|v| v.as_str())
                    {
                        let wt_mgr = crate::worktree::WorktreeManager::new(&working_dir);
                        let target_branch = task
                            .metadata
                            .get("target_branch")
                            .and_then(|v| v.as_str())
                            .unwrap_or("main");
                        let commit_msg = format!(
                            "Merge branch '{}' into '{}' via Plexis Integrator",
                            src_branch, target_branch
                        );
                        if let Ok(sha) =
                            wt_mgr.integrate_branch(src_branch, target_branch, &commit_msg)
                        {
                            task.metadata["integrated_commit_sha"] = serde_json::json!(sha);
                            let _ = self.store.update_task(&task).await;
                        }
                    }

                    // Run Independent Workspace Verification
                    let verif = self
                        .verifier
                        .verify_and_record(&mut task, &execution, &target_dir)
                        .await
                        .map_err(RuntimeError::Storage)?;

                    if verif.verdict == VerificationVerdict::Failed {
                        if let Some(rc) = &self.recovery_controller {
                            let fail_reason = verif
                                .failure_reason
                                .as_deref()
                                .unwrap_or("Verification criteria failed");
                            if let Ok((action, _rec)) = rc
                                .diagnose_and_recover(
                                    &task,
                                    &task.workflow_id,
                                    Some(&execution.id),
                                    task.attempts,
                                    fail_reason,
                                )
                                .await
                            {
                                self.apply_recovery_action(&mut task, action, fail_reason)
                                    .await;
                            }
                        }
                    }
                } else {
                    let reason = result.failure_reason.unwrap_or_else(|| {
                        format!("External agent exited with code {}", result.exit_code)
                    });
                    execution
                        .mark_failed(&reason)
                        .map_err(|e| RuntimeError::InvalidCommand(e.to_string()))?;
                    self.store
                        .update_execution(&execution)
                        .await
                        .map_err(RuntimeError::Storage)?;

                    let _ = task.transition_to(TaskState::Failed);
                    self.store
                        .update_task(&task)
                        .await
                        .map_err(RuntimeError::Storage)?;

                    let fail_evt = Event::new(
                        "execution",
                        execution.id.to_string(),
                        "execution_failed",
                        serde_json::json!({
                            "task_id": task.id.to_string(),
                            "backend": backend.id(),
                            "reason": reason,
                        }),
                    );
                    self.store
                        .append_event(&fail_evt)
                        .await
                        .map_err(RuntimeError::Storage)?;

                    if let Some(rc) = &self.recovery_controller {
                        if let Ok((action, _rec)) = rc
                            .diagnose_and_recover(
                                &task,
                                &task.workflow_id,
                                Some(&execution.id),
                                task.attempts,
                                &reason,
                            )
                            .await
                        {
                            self.apply_recovery_action(&mut task, action, &reason).await;
                        }
                    }
                }
            }
            Err(err) => {
                let reason = err.to_string();
                execution
                    .mark_failed(&reason)
                    .map_err(|e| RuntimeError::InvalidCommand(e.to_string()))?;
                self.store
                    .update_execution(&execution)
                    .await
                    .map_err(RuntimeError::Storage)?;

                let _ = task.transition_to(TaskState::Failed);
                self.store
                    .update_task(&task)
                    .await
                    .map_err(RuntimeError::Storage)?;

                let fail_evt = Event::new(
                    "execution",
                    execution.id.to_string(),
                    "execution_failed",
                    serde_json::json!({
                        "task_id": task.id.to_string(),
                        "backend": backend.id(),
                        "reason": reason,
                    }),
                );
                self.store
                    .append_event(&fail_evt)
                    .await
                    .map_err(RuntimeError::Storage)?;

                if let Some(rc) = &self.recovery_controller {
                    if let Ok((action, _rec)) = rc
                        .diagnose_and_recover(
                            &task,
                            &task.workflow_id,
                            Some(&execution.id),
                            task.attempts,
                            &reason,
                        )
                        .await
                    {
                        self.apply_recovery_action(&mut task, action, &reason).await;
                    }
                }
            }
        }

        // Release agent busy state
        agent.state = AgentState::Idle;
        agent.current_execution_id = None;
        self.store
            .update_agent(&agent)
            .await
            .map_err(RuntimeError::Storage)?;

        // Update command state
        let mut updated_cmd = command.clone();
        if success {
            updated_cmd.mark_completed();
        } else {
            updated_cmd.mark_failed();
        }
        self.store
            .update_command(&updated_cmd)
            .await
            .map_err(RuntimeError::Storage)?;

        Ok(execution)
    }

    async fn apply_recovery_action(
        &self,
        task: &mut plexis_core::Task,
        action: crate::recovery::RecoveryAction,
        reason: &str,
    ) {
        match action {
            crate::recovery::RecoveryAction::MutateStrategy {
                strategy,
                version,
                adjustment,
            } => {
                task.metadata["recovery_advice"] = serde_json::json!({
                    "strategy": strategy,
                    "version": version,
                    "failure": reason,
                    "adjustment": adjustment,
                });
                if strategy == "timeout_adaptation" {
                    let cur = task
                        .metadata
                        .get("timeout_secs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(300);
                    task.metadata["timeout_secs"] =
                        serde_json::json!(cur.saturating_mul(5).max(300));
                }
                task.assigned_agent_id = None;
                let _ = task.transition_to(TaskState::Ready);
                let _ = self.store.update_task(task).await;
            }
            crate::recovery::RecoveryAction::ReassignAgent { suggested_role } => {
                task.metadata["recovery_advice"] = serde_json::json!({
                    "action": "reassign_agent",
                    "suggested_role": suggested_role,
                    "failure": reason,
                });
                task.metadata["suggested_role"] = serde_json::json!(suggested_role);
                task.assigned_agent_id = None;
                let _ = task.transition_to(TaskState::Ready);
                let _ = self.store.update_task(task).await;
            }
            crate::recovery::RecoveryAction::RetryWithBackoff { delay_secs } => {
                task.metadata["recovery_advice"] = serde_json::json!({
                    "action": "retry_with_backoff",
                    "delay_secs": delay_secs,
                    "failure": reason,
                });
                task.assigned_agent_id = None;
                let _ = task.transition_to(TaskState::Ready);
                let _ = self.store.update_task(task).await;
            }
            _ => {}
        }
    }
}
