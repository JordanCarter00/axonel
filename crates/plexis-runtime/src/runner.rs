//! Agent execution engine coordinating sessions, providers, tools, and durable event telemetry.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use plexis_core::ids::{AgentId, TaskId};
use plexis_core::state::{AgentState, TaskState};
use plexis_core::{Agent, Command, CommandType, Event, Execution, Session};
use plexis_providers::{ChatMessage, CompletionRequest, Provider, ToolDefinition};
use plexis_storage::traits::{
    AgentStore, CommandStore, EventStore, ExecutionStore, SessionStore, TaskStore,
    VerificationStore,
};
use plexis_tools::{Sandbox, ToolInvocationContext, ToolRegistry};

use crate::error::RuntimeError;
use crate::verifier::WorkspaceVerifier;

/// Central runner executing an assigned task attempt against an agent and provider.
pub struct AgentRunner<
    S: TaskStore
        + AgentStore
        + SessionStore
        + ExecutionStore
        + CommandStore
        + EventStore
        + VerificationStore
        + 'static,
> {
    store: Arc<S>,
    providers: HashMap<String, Arc<dyn Provider>>,
    tool_registry: ToolRegistry,
    verifier: Arc<WorkspaceVerifier<S>>,
}

impl<
        S: TaskStore
            + AgentStore
            + SessionStore
            + ExecutionStore
            + CommandStore
            + EventStore
            + VerificationStore
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
            tool_registry,
            verifier,
        }
    }

    pub fn register_provider(&mut self, provider: Arc<dyn Provider>) {
        self.providers.insert(provider.id().to_string(), provider);
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
        let tool_defs: Vec<ToolDefinition> = self
            .tool_registry
            .list_tools()
            .into_iter()
            .map(|t| ToolDefinition::new(t.name(), t.description(), t.schema()))
            .collect();

        // 7. Initial message history
        let system_prompt = format!(
            "You are an autonomous Plexis agent named '{}' with role '{}'.\n\
             Task Objective: {}\n\
             Description: {}\n\
             Workspace directory: {}",
            agent.display_name,
            agent.role,
            task.objective,
            task.description.as_deref().unwrap_or("None"),
            working_dir.display()
        );

        let mut messages = vec![
            ChatMessage::system(system_prompt),
            ChatMessage::user(format!(
                "Execute the task objective: '{}'. Use your available tools.",
                task.objective
            )),
        ];

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

                            let tool_ctx = ToolInvocationContext {
                                agent_id: agent.id,
                                execution_id: execution.id,
                                task_id: task.id,
                                arguments: tool_args,
                                sandbox: sandbox.clone(),
                                working_directory: working_dir.clone(),
                            };

                            let (_record, tool_res) =
                                self.tool_registry.invoke(&call.name, &tool_ctx).await;

                            match tool_res {
                                Ok(output) => {
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
                                    let tool_err_str = tool_err.to_string();
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
        if completed_cleanly {
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
            self.verifier
                .verify_and_record(&mut task, &execution, &working_dir)
                .await
                .map_err(RuntimeError::Storage)?;
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
}
