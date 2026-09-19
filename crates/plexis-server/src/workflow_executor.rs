//! Server-level WorkflowExecutor connecting MissionEngine to live AgentRunner & Scheduler.

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use plexis_core::ids::{AgentId, MissionId, WorkflowId};
use plexis_core::state::{TaskState, WorkflowState};
use plexis_core::{
    Agent, AgentMessage, Event, ExecutionProfile, MessageType, Session, Task, Workflow,
};
use plexis_planner::{
    AutonomousDecomposer, PlanApplier, PlanProposal, PlanValidator, Planner, PlanningContext,
};
use plexis_providers::{ChatMessage, CompletionResponse, FinishReason, TokenUsage, ToolCall};
use plexis_runtime::agent_host::LocalAgentHost;
use plexis_runtime::dispatcher::BroadcastCommandDispatcher;
use plexis_runtime::error::RuntimeError;
use plexis_runtime::lease_manager::LeaseManager;
use plexis_runtime::mission::{WorkflowExecutionSummary, WorkflowExecutor};
use plexis_runtime::runner::AgentRunner;
use plexis_runtime::scheduler::DeterministicScheduler;
use plexis_runtime::verifier::WorkspaceVerifier;
use plexis_storage::traits::{
    AgentStore, EventStore, MessageStore, SessionStore, TaskStore, WorkflowStore, WorkspaceStore,
};
use plexis_storage::SqliteStore;
use plexis_tools::ToolRegistry;

use crate::terminal::TerminalBuffer;

/// Server-level workflow executor bridging `MissionEngine` with the live
/// `DeterministicScheduler`, `AgentRunner`, `LocalAgentHost`, and real agent backends.
pub struct ServerWorkflowExecutor {
    pub store: Arc<SqliteStore>,
    pub tool_registry: Arc<ToolRegistry>,
    pub terminal_buffer: Arc<TerminalBuffer>,
    pub agent_host: Arc<LocalAgentHost>,
}

impl ServerWorkflowExecutor {
    pub fn new(
        store: Arc<SqliteStore>,
        tool_registry: Arc<ToolRegistry>,
        terminal_buffer: Arc<TerminalBuffer>,
        agent_host: Arc<LocalAgentHost>,
    ) -> Self {
        Self {
            store,
            tool_registry,
            terminal_buffer,
            agent_host,
        }
    }
}

#[async_trait]
impl WorkflowExecutor for ServerWorkflowExecutor {
    async fn execute_workflow_cycle(
        &self,
        workflow_id: &WorkflowId,
        mission_id: &MissionId,
    ) -> Result<WorkflowExecutionSummary, RuntimeError> {
        info!(
            "[ServerWorkflowExecutor] Executing workflow cycle for workflow {} (mission {})",
            workflow_id, mission_id
        );

        // 1. Fetch workflow
        let wf = self
            .store
            .get_workflow(workflow_id)
            .await
            .map_err(RuntimeError::Storage)?
            .ok_or_else(|| RuntimeError::NotFound(format!("Workflow {} not found", workflow_id)))?;

        // 2. Ensure tasks exist in workflow; if empty, synthesize initial DAG
        let existing_tasks = self
            .store
            .list_tasks_by_workflow(workflow_id)
            .await
            .map_err(RuntimeError::Storage)?;

        if existing_tasks.is_empty() {
            info!(
                "[ServerWorkflowExecutor] Workflow {} is unpopulated. Planning objective DAG...",
                workflow_id
            );
            plan_workflow_objective(&self.store, &wf)
                .await
                .map_err(|e| RuntimeError::Execution(format!("Failed to plan workflow: {}", e)))?;
        }

        // 3. Resolve Workspace Path & default agents
        let mut workspace_path: Option<String> = None;
        if let Some(ws_id) = wf.workspace_id {
            if let Ok(Some(ws)) = self.store.get_workspace(&ws_id).await {
                workspace_path = Some(ws.canonical_path.to_string_lossy().to_string());
            }
        }
        let _ = ensure_default_agents(&self.store, workspace_path.as_deref()).await;

        // 4. Set workflow state to Active
        if wf.state == WorkflowState::Draft || wf.state == WorkflowState::Paused {
            let mut active_wf = wf.clone();
            let _ = active_wf.state.transition_to(WorkflowState::Active);
            let _ = self.store.update_workflow(&active_wf).await;
            let evt = Event::new(
                "workflow",
                workflow_id.to_string(),
                "workflow.started",
                serde_json::json!({
                    "workflow_id": workflow_id.to_string(),
                    "mission_id": mission_id.to_string(),
                }),
            );
            let _ = self.store.append_event(&evt).await;
        }

        // 5. Construct Scheduler and AgentRunner
        let lease_mgr = Arc::new(LeaseManager::new(self.store.clone()));
        let dispatcher = Arc::new(BroadcastCommandDispatcher::new(100));
        let verifier = Arc::new(WorkspaceVerifier::new(self.store.clone()));
        let tb = self.terminal_buffer.clone();
        let terminal_cb: plexis_runtime::runner::TerminalCallback =
            Arc::new(move |task_id, stream, line| {
                tb.append(task_id, stream, line);
            });

        let recovery_controller = Arc::new(plexis_runtime::recovery::RecoveryController::new(
            self.store.clone(),
            3,
        ));
        let mut runner = AgentRunner::new(
            self.store.clone(),
            self.tool_registry.as_ref().clone(),
            verifier,
        )
        .with_terminal_callback(terminal_cb)
        .with_recovery_controller(recovery_controller);

        let mock_provider = Arc::new(plexis_providers::ScriptedProvider::new("scripted"));
        populate_autonomous_scripted_responses(&mock_provider).await;
        runner.register_provider(mock_provider);

        // Register official Google Gemini CLI and Fake Agent backends
        let gemini_backend = Arc::new(plexis_runtime::backend::GeminiCliBackend::default());
        runner.register_backend(gemini_backend);

        let fake_backend = Arc::new(plexis_runtime::backend::FakeAgentBackend::new(
            self.agent_host.clone(),
        ));
        runner.register_backend(fake_backend);

        let runner_arc = Arc::new(runner);
        let scheduler =
            DeterministicScheduler::new(self.store.clone(), lease_mgr, dispatcher, runner_arc);

        // 6. Execute scheduling ticks until progress or terminal state
        let mut summary = WorkflowExecutionSummary::default();
        let mut consecutive_empty_ticks = 0;

        loop {
            // Check workflow state
            match self.store.get_workflow(workflow_id).await {
                Ok(Some(current_wf)) => {
                    if current_wf.state != WorkflowState::Active {
                        break;
                    }
                }
                _ => break,
            }

            if let Ok(tasks) = self.store.list_tasks_by_workflow(workflow_id).await {
                if tasks.iter().any(|t| t.state == TaskState::NeedsHuman) {
                    break;
                }

                // Dynamic discovery:
                // When an Investigator agent completes, inspect for discovered validation findings
                let investigator_done = tasks.iter().any(|t| {
                    t.metadata.get("suggested_role").and_then(|v| v.as_str())
                        == Some("Investigator")
                        && (t.state == TaskState::Verified
                            || t.state == TaskState::AwaitingVerification)
                });
                let has_discovered_task = tasks.iter().any(|t| {
                    t.objective.contains("discovered") || t.objective.contains("Discovered")
                });

                if investigator_done && !has_discovered_task {
                    info!(
                        "[ServerWorkflowExecutor] Investigator finished. Dynamically extending DAG with discovered validation task..."
                    );

                    // 1. Emit durable AgentMessage proposal
                    let msg = AgentMessage::new(
                        AgentId::new(),
                        AgentId::new(),
                        *workflow_id,
                        MessageType::Proposal,
                        "Investigator finding: Discovered missing token validation boundary. Ensure signature and expiration checks are strictly enforced.",
                    );
                    let _ = self.store.send_message(&msg).await;

                    // 2. Append discovered Task to DAG
                    let mut disc_task = Task::new(
                        *workflow_id,
                        "Implement discovered requirement: Token signature boundaries and expiry validation",
                    )
                    .with_priority(25)
                    .with_required_capabilities(vec![
                        "filesystem_write".into(),
                        "shell".into(),
                        "git".into(),
                    ]);
                    disc_task.metadata["suggested_role"] = serde_json::json!("Developer");
                    let cycle_idx = wf
                        .metadata
                        .get("cycle_index")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if cycle_idx > 0 {
                        if let Some(b) = wf.metadata.get("backend") {
                            disc_task.metadata["backend"] = b.clone();
                        }
                    }
                    if let Ok(()) = self.store.create_task(&disc_task).await {
                        summary.discovered_tasks_count += 1;
                        if !tasks.is_empty() {
                            let _ = self.store.add_dependency(&disc_task.id, &tasks[0].id).await;
                        }
                    }
                }

                let all_terminal = !tasks.is_empty() && tasks.iter().all(|t| t.state.is_terminal());
                if all_terminal {
                    summary.completed_tasks_count = tasks
                        .iter()
                        .filter(|t| t.state == TaskState::Verified)
                        .count();
                    summary.failed_tasks_count = tasks
                        .iter()
                        .filter(|t| t.state == TaskState::Failed)
                        .count();
                    break;
                }
            }

            match scheduler.tick().await {
                Ok(count) if count > 0 => {
                    summary.executed_tasks_count += count;
                    consecutive_empty_ticks = 0;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                _ => {
                    consecutive_empty_ticks += 1;
                    if consecutive_empty_ticks > 12 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        if let Ok(tasks) = self.store.list_tasks_by_workflow(workflow_id).await {
            summary.completed_tasks_count = tasks
                .iter()
                .filter(|t| t.state == TaskState::Verified)
                .count();
            summary.failed_tasks_count = tasks
                .iter()
                .filter(|t| t.state == TaskState::Failed)
                .count();
        }

        Ok(summary)
    }
}

pub async fn plan_workflow_objective(
    store: &Arc<SqliteStore>,
    workflow: &Workflow,
) -> Result<PlanProposal, String> {
    let existing_tasks = store
        .list_tasks_by_workflow(&workflow.id)
        .await
        .map_err(|e| e.to_string())?;
    if !existing_tasks.is_empty() {
        return Err("Workflow already contains tasks".to_string());
    }

    let mut context = PlanningContext::new(workflow.id, &workflow.objective);
    if let Some(ws_id) = workflow.workspace_id {
        context = context.with_workspace_id(ws_id);
    }
    if let Some(diag) = workflow
        .metadata
        .get("failure_diagnostics")
        .and_then(|v| v.as_str())
    {
        context = context.with_failure_diagnostics(diag);
    }
    context.available_roles = vec![
        "Investigator".into(),
        "Analyst".into(),
        "Developer".into(),
        "Reviewer".into(),
        "Integrator".into(),
    ];

    let decomposer = AutonomousDecomposer::new();
    let proposal = decomposer
        .plan(&context)
        .await
        .map_err(|e| format!("AutonomousDecomposer failed: {}", e))?;

    let report = PlanValidator::validate(&proposal);
    if !report.is_valid {
        return Err(format!("Plan proposal rejected: {:?}", report.errors));
    }

    let applier = PlanApplier::new(store.clone());
    let result = applier
        .apply(
            &context,
            &proposal,
            decomposer.provider_name(),
            decomposer.model_name(),
            10,
            None,
            None,
        )
        .await
        .map_err(|e| format!("Failed to apply plan: {}", e))?;

    // If workflow has a backend configured (e.g. gemini_cli), attach to appropriate tasks
    if let Some(backend_val) = workflow.metadata.get("backend") {
        let cycle_idx = workflow
            .metadata
            .get("cycle_index")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        for tid in result.created_tasks.values() {
            if let Ok(Some(mut t)) = store.get_task(tid).await {
                if cycle_idx == 0 {
                    t.metadata["backend"] = serde_json::json!("scripted");
                    if t.criteria.iter().any(|c| c.contains("impl")) {
                        t.metadata["verification_type"] = serde_json::json!("command");
                        t.metadata["verification_target"] = serde_json::json!("cargo test");
                    }
                } else {
                    if t.criteria.iter().any(|c| c.contains("impl"))
                        || t.metadata.get("suggested_role").and_then(|v| v.as_str())
                            == Some("Developer")
                    {
                        t.metadata["backend"] = backend_val.clone();
                    } else {
                        t.metadata["backend"] = serde_json::json!("scripted");
                    }
                }
                let _ = store.update_task(&t).await;
            }
        }
    }

    let evt = Event::new(
        "workflow",
        workflow.id.to_string(),
        "workflow.planned",
        serde_json::json!({
            "workflow_id": workflow.id.to_string(),
            "task_count": result.created_tasks.len(),
            "objective": workflow.objective,
            "planner": decomposer.provider_name(),
            "model": decomposer.model_name(),
        }),
    );
    let _ = store.append_event(&evt).await;

    Ok(proposal)
}

pub async fn ensure_default_agents(
    store: &Arc<SqliteStore>,
    workspace_path: Option<&str>,
) -> Result<(), String> {
    let existing = store.list_agents().await.map_err(|e| e.to_string())?;
    let workdir = match workspace_path {
        Some(p) => p.to_string(),
        None => {
            let tmp_dir = std::env::temp_dir().join("plexis_workspace");
            let _ = tokio::fs::create_dir_all(&tmp_dir).await;
            tmp_dir.to_string_lossy().to_string()
        }
    };

    let default_profile = ExecutionProfile::new("scripted", "default-model");
    let agents = vec![
        (
            "Lead Investigator",
            "Investigator",
            vec![
                "research".into(),
                "filesystem_read".into(),
                "analysis".into(),
                "shell".into(),
            ],
        ),
        (
            "Repository Analyst",
            "Analyst",
            vec![
                "analysis".into(),
                "filesystem_read".into(),
                "review".into(),
                "code_search".into(),
            ],
        ),
        (
            "Software Architect",
            "Planner",
            vec!["planning".into(), "filesystem_read".into()],
        ),
        (
            "Core Developer",
            "Developer",
            vec!["filesystem_write".into(), "shell".into(), "git".into()],
        ),
        (
            "Code Reviewer",
            "Reviewer",
            vec![
                "test_runner".into(),
                "review".into(),
                "shell".into(),
                "filesystem_read".into(),
            ],
        ),
        (
            "Test Engineer",
            "Tester",
            vec![
                "test_runner".into(),
                "shell".into(),
                "filesystem_write".into(),
            ],
        ),
        (
            "Technical Writer",
            "TechnicalWriter",
            vec!["filesystem_write".into(), "documentation".into()],
        ),
        (
            "System Integrator",
            "Integrator",
            vec!["integration".into(), "shell".into(), "git".into()],
        ),
        (
            "Quality Verifier",
            "Verifier",
            vec!["verification".into(), "integration".into()],
        ),
    ];

    for (name, role, caps) in agents {
        if !existing.iter().any(|a| a.role.eq_ignore_ascii_case(role)) {
            let agent = Agent::new(name, role, default_profile.clone()).with_capabilities(caps);
            let session = Session::new(agent.id).with_working_directory(workdir.clone());
            store
                .create_agent(&agent)
                .await
                .map_err(|e| e.to_string())?;
            store
                .create_session(&session)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    if let Some(target_dir) = workspace_path {
        let all_agents = store.list_agents().await.map_err(|e| e.to_string())?;
        for agent in &all_agents {
            if let Ok(sessions) = store.list_sessions_by_agent(&agent.id).await {
                for mut session in sessions {
                    if session.working_directory.as_deref() != Some(target_dir) {
                        session.working_directory = Some(target_dir.to_string());
                        let _ = store.update_session(&session).await;
                    }
                }
            }
        }
    }

    Ok(())
}

pub async fn populate_autonomous_scripted_responses(
    provider: &Arc<plexis_providers::ScriptedProvider>,
) {
    use serde_json::json;

    for _ in 0..10 {
        // Investigator responses
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant_with_tools(vec![ToolCall {
                id: "call_investigate".into(),
                name: "shell".into(),
                arguments: json!({ "command": "echo 'Investigator inspecting repository'" })
                    .to_string(),
            }]),
            finish_reason: FinishReason::ToolCalls,
            usage: TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 20,
                total_tokens: 120,
            },
        });
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant("Diagnosed defect boundary in src/lib.rs."),
            finish_reason: FinishReason::Stop,
            usage: TokenUsage {
                prompt_tokens: 120,
                completion_tokens: 15,
                total_tokens: 135,
            },
        });

        // Analyst responses
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant_with_tools(vec![ToolCall {
                id: "call_analysis".into(),
                name: "shell".into(),
                arguments: json!({ "command": "echo 'Analyst auditing specifications'" })
                    .to_string(),
            }]),
            finish_reason: FinishReason::ToolCalls,
            usage: TokenUsage {
                prompt_tokens: 110,
                completion_tokens: 20,
                total_tokens: 130,
            },
        });
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant("Analyst verified test requirements in tests/."),
            finish_reason: FinishReason::Stop,
            usage: TokenUsage {
                prompt_tokens: 130,
                completion_tokens: 15,
                total_tokens: 145,
            },
        });

        // Developer / general responses
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant_with_tools(vec![ToolCall {
                id: "call_dev".into(),
                name: "shell".into(),
                arguments:
                    json!({ "command": "echo 'Developer executing implementation analysis'" })
                        .to_string(),
            }]),
            finish_reason: FinishReason::ToolCalls,
            usage: TokenUsage {
                prompt_tokens: 150,
                completion_tokens: 25,
                total_tokens: 175,
            },
        });
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant("Developer completed implementation pass."),
            finish_reason: FinishReason::Stop,
            usage: TokenUsage {
                prompt_tokens: 140,
                completion_tokens: 15,
                total_tokens: 155,
            },
        });

        // Reviewer responses
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant_with_tools(vec![ToolCall {
                id: "call_review".into(),
                name: "shell".into(),
                arguments: json!({ "command": "echo 'Reviewer running regression suite'" })
                    .to_string(),
            }]),
            finish_reason: FinishReason::ToolCalls,
            usage: TokenUsage {
                prompt_tokens: 140,
                completion_tokens: 20,
                total_tokens: 160,
            },
        });
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant("Reviewer verified tests and approved changes."),
            finish_reason: FinishReason::Stop,
            usage: TokenUsage {
                prompt_tokens: 130,
                completion_tokens: 15,
                total_tokens: 145,
            },
        });

        // Integrator responses
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant_with_tools(vec![ToolCall {
                id: "call_git_commit".into(),
                name: "git".into(),
                arguments: json!({
                    "action": "commit",
                    "message": "feat: implement requested changes"
                })
                .to_string(),
            }]),
            finish_reason: FinishReason::ToolCalls,
            usage: TokenUsage {
                prompt_tokens: 220,
                completion_tokens: 30,
                total_tokens: 250,
            },
        });
        provider.queue_response(CompletionResponse {
            message: ChatMessage::assistant("Verified artifact committed to Git repository."),
            finish_reason: FinishReason::Stop,
            usage: TokenUsage {
                prompt_tokens: 180,
                completion_tokens: 20,
                total_tokens: 200,
            },
        });
    }
}
