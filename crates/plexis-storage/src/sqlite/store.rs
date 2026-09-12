//! SQLite implementation of the Plexis storage repositories.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Arc;
use tokio::sync::Mutex;

use plexis_core::ids::{AgentId, CommandId, EventId, LeaseId, TaskId, WorkflowId};
use plexis_core::state::{AgentState, CommandState, TaskState, WorkflowState};
use plexis_core::{
    Agent, Command, CommandTarget, CommandType, Event, ExecutionProfile, Lease, Task, TaskGraph,
    Workflow,
};

use crate::error::StorageError;
use crate::sqlite::migrations::run_migrations;
use crate::traits::{AgentStore, CommandStore, EventStore, LeaseStore, TaskStore, WorkflowStore};

/// Primary SQLite-backed storage manager for Plexis.
#[derive(Clone)]
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Opens or creates a SQLite database file at `path` and runs migrations.
    pub fn open(path: &str) -> Result<Self, StorageError> {
        let mut conn = Connection::open(path)?;
        Self::configure_and_migrate(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Opens an in-memory SQLite database, ideal for testing and ephemeral workloads.
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure_and_migrate(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn configure_and_migrate(conn: &mut Connection) -> Result<(), StorageError> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )?;
        run_migrations(conn)?;
        Ok(())
    }
}

#[async_trait]
impl WorkflowStore for SqliteStore {
    async fn create_workflow(&self, wf: &Workflow) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO workflows (id, title, objective, state, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                wf.id.to_string(),
                wf.title,
                wf.objective,
                serde_json::to_string(&wf.state)?,
                serde_json::to_string(&wf.metadata)?,
                wf.created_at.to_rfc3339(),
                wf.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn get_workflow(&self, id: &WorkflowId) -> Result<Option<Workflow>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, title, objective, state, metadata, created_at, updated_at
             FROM workflows WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let title: String = row.get(1)?;
                let objective: String = row.get(2)?;
                let state_str: String = row.get(3)?;
                let meta_str: String = row.get(4)?;
                let created_str: String = row.get(5)?;
                let updated_str: String = row.get(6)?;

                Ok((
                    id_str,
                    title,
                    objective,
                    state_str,
                    meta_str,
                    created_str,
                    updated_str,
                ))
            })
            .optional()?;

        match result {
            Some((id_str, title, objective, state_str, meta_str, created_str, updated_str)) => {
                let wf_id: WorkflowId = id_str.parse()?;
                let state: WorkflowState = serde_json::from_str(&state_str)?;
                let metadata: serde_json::Value = serde_json::from_str(&meta_str)?;
                let created_at = DateTime::parse_from_rfc3339(&created_str)
                    .map_err(|e| StorageError::Migration(e.to_string()))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                    .map_err(|e| StorageError::Migration(e.to_string()))?
                    .with_timezone(&Utc);

                Ok(Some(Workflow {
                    id: wf_id,
                    title,
                    objective,
                    state,
                    metadata,
                    created_at,
                    updated_at,
                }))
            }
            None => Ok(None),
        }
    }

    async fn update_workflow(&self, wf: &Workflow) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "UPDATE workflows
             SET title = ?1, objective = ?2, state = ?3, metadata = ?4, updated_at = ?5
             WHERE id = ?6",
            params![
                wf.title,
                wf.objective,
                serde_json::to_string(&wf.state)?,
                serde_json::to_string(&wf.metadata)?,
                wf.updated_at.to_rfc3339(),
                wf.id.to_string(),
            ],
        )?;

        if rows == 0 {
            return Err(StorageError::NotFound {
                entity_type: "Workflow",
                id: wf.id.to_string(),
            });
        }
        Ok(())
    }

    async fn list_workflows(&self) -> Result<Vec<Workflow>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, title, objective, state, metadata, created_at, updated_at
             FROM workflows ORDER BY created_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            let id_str: String = row.get(0)?;
            let title: String = row.get(1)?;
            let objective: String = row.get(2)?;
            let state_str: String = row.get(3)?;
            let meta_str: String = row.get(4)?;
            let created_str: String = row.get(5)?;
            let updated_str: String = row.get(6)?;

            Ok((
                id_str,
                title,
                objective,
                state_str,
                meta_str,
                created_str,
                updated_str,
            ))
        })?;

        let mut workflows = Vec::new();
        for r in rows {
            let (id_str, title, objective, state_str, meta_str, created_str, updated_str) = r?;
            let wf_id: WorkflowId = id_str.parse()?;
            let state: WorkflowState = serde_json::from_str(&state_str)?;
            let metadata: serde_json::Value = serde_json::from_str(&meta_str)?;
            let created_at = DateTime::parse_from_rfc3339(&created_str)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&updated_str)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc);

            workflows.push(Workflow {
                id: wf_id,
                title,
                objective,
                state,
                metadata,
                created_at,
                updated_at,
            });
        }

        Ok(workflows)
    }
}

#[async_trait]
impl TaskStore for SqliteStore {
    async fn create_task(&self, task: &Task) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO tasks (
                id, workflow_id, objective, description, parent_id, state,
                priority, criteria, assigned_agent_id, attempts, max_attempts,
                metadata, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                task.id.to_string(),
                task.workflow_id.to_string(),
                task.objective,
                task.description,
                task.parent_id.map(|id| id.to_string()),
                serde_json::to_string(&task.state)?,
                task.priority,
                serde_json::to_string(&task.criteria)?,
                task.assigned_agent_id.map(|id| id.to_string()),
                task.attempts,
                task.max_attempts,
                serde_json::to_string(&task.metadata)?,
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn get_task(&self, id: &TaskId) -> Result<Option<Task>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, workflow_id, objective, description, parent_id, state,
                    priority, criteria, assigned_agent_id, attempts, max_attempts,
                    metadata, created_at, updated_at
             FROM tasks WHERE id = ?1",
        )?;

        let row = stmt
            .query_row(params![id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i32>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, u32>(9)?,
                    row.get::<_, u32>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                ))
            })
            .optional()?;

        match row {
            Some(t) => Ok(Some(parse_task_tuple(t)?)),
            None => Ok(None),
        }
    }

    async fn update_task(&self, task: &Task) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "UPDATE tasks SET
                objective = ?1, description = ?2, parent_id = ?3, state = ?4,
                priority = ?5, criteria = ?6, assigned_agent_id = ?7, attempts = ?8,
                max_attempts = ?9, metadata = ?10, updated_at = ?11
             WHERE id = ?12",
            params![
                task.objective,
                task.description,
                task.parent_id.map(|id| id.to_string()),
                serde_json::to_string(&task.state)?,
                task.priority,
                serde_json::to_string(&task.criteria)?,
                task.assigned_agent_id.map(|id| id.to_string()),
                task.attempts,
                task.max_attempts,
                serde_json::to_string(&task.metadata)?,
                task.updated_at.to_rfc3339(),
                task.id.to_string(),
            ],
        )?;

        if rows == 0 {
            return Err(StorageError::NotFound {
                entity_type: "Task",
                id: task.id.to_string(),
            });
        }
        Ok(())
    }

    async fn list_tasks_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<Vec<Task>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, workflow_id, objective, description, parent_id, state,
                    priority, criteria, assigned_agent_id, attempts, max_attempts,
                    metadata, created_at, updated_at
             FROM tasks WHERE workflow_id = ?1 ORDER BY priority DESC, created_at ASC",
        )?;

        let rows = stmt.query_map(params![workflow_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i32>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, u32>(9)?,
                row.get::<_, u32>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
            ))
        })?;

        let mut tasks = Vec::new();
        for r in rows {
            tasks.push(parse_task_tuple(r?)?);
        }

        Ok(tasks)
    }

    async fn add_dependency(
        &self,
        task_id: &TaskId,
        depends_on_id: &TaskId,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR IGNORE INTO task_dependencies (task_id, depends_on_id) VALUES (?1, ?2)",
            params![task_id.to_string(), depends_on_id.to_string()],
        )?;
        Ok(())
    }

    async fn get_dependencies(&self, task_id: &TaskId) -> Result<Vec<TaskId>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt =
            conn.prepare("SELECT depends_on_id FROM task_dependencies WHERE task_id = ?1")?;

        let rows = stmt.query_map(params![task_id.to_string()], |row| {
            let s: String = row.get(0)?;
            Ok(s)
        })?;

        let mut ids = Vec::new();
        for r in rows {
            let s = r?;
            ids.push(s.parse()?);
        }

        Ok(ids)
    }

    async fn get_dependents(&self, task_id: &TaskId) -> Result<Vec<TaskId>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt =
            conn.prepare("SELECT task_id FROM task_dependencies WHERE depends_on_id = ?1")?;

        let rows = stmt.query_map(params![task_id.to_string()], |row| {
            let s: String = row.get(0)?;
            Ok(s)
        })?;

        let mut ids = Vec::new();
        for r in rows {
            let s = r?;
            ids.push(s.parse()?);
        }

        Ok(ids)
    }

    async fn load_task_graph(&self, workflow_id: &WorkflowId) -> Result<TaskGraph, StorageError> {
        let tasks = self.list_tasks_by_workflow(workflow_id).await?;
        let conn = self.conn.lock().await;

        let mut stmt = conn.prepare(
            "SELECT td.task_id, td.depends_on_id
             FROM task_dependencies td
             JOIN tasks t ON td.task_id = t.id
             WHERE t.workflow_id = ?1",
        )?;

        let edge_rows = stmt.query_map(params![workflow_id.to_string()], |row| {
            let t: String = row.get(0)?;
            let d: String = row.get(1)?;
            Ok((t, d))
        })?;

        let mut graph = TaskGraph::new();
        for task in tasks {
            graph.add_task(task);
        }

        for edge in edge_rows {
            let (task_str, dep_str) = edge?;
            let t_id: TaskId = task_str.parse()?;
            let d_id: TaskId = dep_str.parse()?;
            graph
                .add_dependency(t_id, d_id)
                .map_err(|e| StorageError::Migration(e.to_string()))?;
        }

        Ok(graph)
    }
}

type TaskTuple = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    i32,
    String,
    Option<String>,
    u32,
    u32,
    String,
    String,
    String,
);

fn parse_task_tuple(t: TaskTuple) -> Result<Task, StorageError> {
    let (
        id_str,
        wf_str,
        objective,
        description,
        parent_str,
        state_str,
        priority,
        criteria_str,
        agent_str,
        attempts,
        max_attempts,
        meta_str,
        created_str,
        updated_str,
    ) = t;

    let id: TaskId = id_str.parse()?;
    let workflow_id: WorkflowId = wf_str.parse()?;
    let parent_id = match parent_str {
        Some(s) => Some(s.parse()?),
        None => None,
    };
    let state: TaskState = serde_json::from_str(&state_str)?;
    let criteria: Vec<String> = serde_json::from_str(&criteria_str)?;
    let assigned_agent_id = match agent_str {
        Some(s) => Some(s.parse()?),
        None => None,
    };
    let metadata: serde_json::Value = serde_json::from_str(&meta_str)?;
    let created_at = DateTime::parse_from_rfc3339(&created_str)
        .map_err(|e| StorageError::Migration(e.to_string()))?
        .with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&updated_str)
        .map_err(|e| StorageError::Migration(e.to_string()))?
        .with_timezone(&Utc);

    Ok(Task {
        id,
        workflow_id,
        objective,
        description,
        parent_id,
        state,
        priority,
        criteria,
        assigned_agent_id,
        attempts,
        max_attempts,
        metadata,
        created_at,
        updated_at,
    })
}

#[async_trait]
impl AgentStore for SqliteStore {
    async fn create_agent(&self, agent: &Agent) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO agents (
                id, display_name, role, provider, model, parameters,
                capabilities, permissions, state, current_execution_id,
                configuration, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                agent.id.to_string(),
                agent.display_name,
                agent.role,
                agent.provider_profile.provider,
                agent.provider_profile.model,
                serde_json::to_string(&agent.provider_profile.parameters)?,
                serde_json::to_string(&agent.capabilities)?,
                serde_json::to_string(&agent.permissions)?,
                serde_json::to_string(&agent.state)?,
                agent.current_execution_id.map(|id| id.to_string()),
                serde_json::to_string(&agent.configuration)?,
                agent.created_at.to_rfc3339(),
                agent.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn get_agent(&self, id: &AgentId) -> Result<Option<Agent>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, display_name, role, provider, model, parameters,
                    capabilities, permissions, state, current_execution_id,
                    configuration, created_at, updated_at
             FROM agents WHERE id = ?1",
        )?;

        let row = stmt
            .query_row(params![id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                ))
            })
            .optional()?;

        match row {
            Some(r) => {
                let id: AgentId = r.0.parse()?;
                let display_name = r.1;
                let role = r.2;
                let provider_profile = ExecutionProfile {
                    provider: r.3,
                    model: r.4,
                    parameters: serde_json::from_str(&r.5)?,
                };
                let capabilities: Vec<String> = serde_json::from_str(&r.6)?;
                let permissions: Vec<String> = serde_json::from_str(&r.7)?;
                let state: AgentState = serde_json::from_str(&r.8)?;
                let current_execution_id = match r.9 {
                    Some(s) => Some(s.parse()?),
                    None => None,
                };
                let configuration: serde_json::Value = serde_json::from_str(&r.10)?;
                let created_at = DateTime::parse_from_rfc3339(&r.11)
                    .map_err(|e| StorageError::Migration(e.to_string()))?
                    .with_timezone(&Utc);
                let updated_at = DateTime::parse_from_rfc3339(&r.12)
                    .map_err(|e| StorageError::Migration(e.to_string()))?
                    .with_timezone(&Utc);

                Ok(Some(Agent {
                    id,
                    display_name,
                    role,
                    provider_profile,
                    capabilities,
                    permissions,
                    state,
                    current_execution_id,
                    configuration,
                    created_at,
                    updated_at,
                }))
            }
            None => Ok(None),
        }
    }

    async fn update_agent(&self, agent: &Agent) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "UPDATE agents SET
                display_name = ?1, role = ?2, provider = ?3, model = ?4,
                parameters = ?5, capabilities = ?6, permissions = ?7,
                state = ?8, current_execution_id = ?9, configuration = ?10,
                updated_at = ?11
             WHERE id = ?12",
            params![
                agent.display_name,
                agent.role,
                agent.provider_profile.provider,
                agent.provider_profile.model,
                serde_json::to_string(&agent.provider_profile.parameters)?,
                serde_json::to_string(&agent.capabilities)?,
                serde_json::to_string(&agent.permissions)?,
                serde_json::to_string(&agent.state)?,
                agent.current_execution_id.map(|id| id.to_string()),
                serde_json::to_string(&agent.configuration)?,
                agent.updated_at.to_rfc3339(),
                agent.id.to_string(),
            ],
        )?;

        if rows == 0 {
            return Err(StorageError::NotFound {
                entity_type: "Agent",
                id: agent.id.to_string(),
            });
        }
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<Agent>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, display_name, role, provider, model, parameters,
                    capabilities, permissions, state, current_execution_id,
                    configuration, created_at, updated_at
             FROM agents ORDER BY created_at ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
            ))
        })?;

        let mut agents = Vec::new();
        for r in rows {
            let r = r?;
            let id: AgentId = r.0.parse()?;
            let display_name = r.1;
            let role = r.2;
            let provider_profile = ExecutionProfile {
                provider: r.3,
                model: r.4,
                parameters: serde_json::from_str(&r.5)?,
            };
            let capabilities: Vec<String> = serde_json::from_str(&r.6)?;
            let permissions: Vec<String> = serde_json::from_str(&r.7)?;
            let state: AgentState = serde_json::from_str(&r.8)?;
            let current_execution_id = match r.9 {
                Some(s) => Some(s.parse()?),
                None => None,
            };
            let configuration: serde_json::Value = serde_json::from_str(&r.10)?;
            let created_at = DateTime::parse_from_rfc3339(&r.11)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc);
            let updated_at = DateTime::parse_from_rfc3339(&r.12)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc);

            agents.push(Agent {
                id,
                display_name,
                role,
                provider_profile,
                capabilities,
                permissions,
                state,
                current_execution_id,
                configuration,
                created_at,
                updated_at,
            });
        }

        Ok(agents)
    }
}

#[async_trait]
impl CommandStore for SqliteStore {
    async fn enqueue_command(&self, cmd: &Command) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;

        let (target_type, target_id) = match &cmd.target {
            CommandTarget::Agent(id) => ("agent", Some(id.to_string())),
            CommandTarget::Task(id) => ("task", Some(id.to_string())),
            CommandTarget::Workflow(id) => ("workflow", Some(id.to_string())),
            CommandTarget::System => ("system", None),
        };

        let result = conn.execute(
            "INSERT INTO commands (
                id, target_type, target_id, command_type, payload,
                state, idempotency_key, attempts, max_attempts,
                created_at, dispatched_at, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                cmd.id.to_string(),
                target_type,
                target_id,
                serde_json::to_string(&cmd.command_type)?,
                serde_json::to_string(&cmd.payload)?,
                serde_json::to_string(&cmd.state)?,
                cmd.idempotency_key,
                cmd.attempts,
                cmd.max_attempts,
                cmd.created_at.to_rfc3339(),
                cmd.dispatched_at.map(|d| d.to_rfc3339()),
                cmd.completed_at.map(|c| c.to_rfc3339()),
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(StorageError::IdempotencyConflict(
                    cmd.idempotency_key.clone(),
                ))
            }
            Err(e) => Err(StorageError::Sqlite(e)),
        }
    }

    async fn get_command(&self, id: &CommandId) -> Result<Option<Command>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, target_type, target_id, command_type, payload,
                    state, idempotency_key, attempts, max_attempts,
                    created_at, dispatched_at, completed_at
             FROM commands WHERE id = ?1",
        )?;

        let row = stmt
            .query_row(params![id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, u32>(7)?,
                    row.get::<_, u32>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                ))
            })
            .optional()?;

        match row {
            Some(r) => Ok(Some(parse_command_tuple(r)?)),
            None => Ok(None),
        }
    }

    async fn get_by_idempotency_key(&self, key: &str) -> Result<Option<Command>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, target_type, target_id, command_type, payload,
                    state, idempotency_key, attempts, max_attempts,
                    created_at, dispatched_at, completed_at
             FROM commands WHERE idempotency_key = ?1",
        )?;

        let row = stmt
            .query_row(params![key], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, u32>(7)?,
                    row.get::<_, u32>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                ))
            })
            .optional()?;

        match row {
            Some(r) => Ok(Some(parse_command_tuple(r)?)),
            None => Ok(None),
        }
    }

    async fn claim_next_queued_command(&self) -> Result<Option<Command>, StorageError> {
        let conn = self.conn.lock().await;

        let queued_id: Option<String> = conn
            .query_row(
                "SELECT id FROM commands WHERE state = '\"queued\"' ORDER BY created_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;

        let Some(cmd_id) = queued_id else {
            return Ok(None);
        };

        let now = Utc::now();
        conn.execute(
            "UPDATE commands SET
                state = '\"dispatched\"',
                attempts = attempts + 1,
                dispatched_at = ?1
             WHERE id = ?2",
            params![now.to_rfc3339(), cmd_id],
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, target_type, target_id, command_type, payload,
                    state, idempotency_key, attempts, max_attempts,
                    created_at, dispatched_at, completed_at
             FROM commands WHERE id = ?1",
        )?;

        let row = stmt.query_row(params![cmd_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, u32>(7)?,
                row.get::<_, u32>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
            ))
        })?;

        Ok(Some(parse_command_tuple(row)?))
    }

    async fn update_command(&self, cmd: &Command) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE commands SET
                state = ?1, attempts = ?2, dispatched_at = ?3, completed_at = ?4
             WHERE id = ?5",
            params![
                serde_json::to_string(&cmd.state)?,
                cmd.attempts,
                cmd.dispatched_at.map(|d| d.to_rfc3339()),
                cmd.completed_at.map(|c| c.to_rfc3339()),
                cmd.id.to_string(),
            ],
        )?;
        Ok(())
    }
}

type CommandTuple = (
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    u32,
    u32,
    String,
    Option<String>,
    Option<String>,
);

fn parse_command_tuple(r: CommandTuple) -> Result<Command, StorageError> {
    let id: CommandId = r.0.parse()?;
    let target = match r.1.as_str() {
        "agent" => CommandTarget::Agent(r.2.unwrap_or_default().parse()?),
        "task" => CommandTarget::Task(r.2.unwrap_or_default().parse()?),
        "workflow" => CommandTarget::Workflow(r.2.unwrap_or_default().parse()?),
        _ => CommandTarget::System,
    };
    let command_type: CommandType = serde_json::from_str(&r.3)?;
    let payload: serde_json::Value = serde_json::from_str(&r.4)?;
    let state: CommandState = serde_json::from_str(&r.5)?;
    let idempotency_key = r.6;
    let attempts = r.7;
    let max_attempts = r.8;
    let created_at = DateTime::parse_from_rfc3339(&r.9)
        .map_err(|e| StorageError::Migration(e.to_string()))?
        .with_timezone(&Utc);
    let dispatched_at = match r.10 {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(&s)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc),
        ),
        None => None,
    };
    let completed_at = match r.11 {
        Some(s) => Some(
            DateTime::parse_from_rfc3339(&s)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc),
        ),
        None => None,
    };

    Ok(Command {
        id,
        target,
        command_type,
        payload,
        state,
        idempotency_key,
        attempts,
        max_attempts,
        created_at,
        dispatched_at,
        completed_at,
    })
}

#[async_trait]
impl LeaseStore for SqliteStore {
    async fn acquire_lease(&self, lease: &Lease) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;

        // Check if existing lease on task is still active
        let existing = conn
            .query_row(
                "SELECT id, expires_at, generation FROM leases WHERE task_id = ?1",
                params![lease.task_id.to_string()],
                |row| {
                    let id: String = row.get(0)?;
                    let exp: String = row.get(1)?;
                    let gen: u64 = row.get(2)?;
                    Ok((id, exp, gen))
                },
            )
            .optional()?;

        let now = Utc::now();
        if let Some((old_id, exp_str, _)) = existing {
            let exp = DateTime::parse_from_rfc3339(&exp_str)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc);

            if exp > now {
                return Err(StorageError::LeaseConflict(lease.task_id.to_string()));
            } else {
                // Delete stale lease
                conn.execute("DELETE FROM leases WHERE id = ?1", params![old_id])?;
            }
        }

        conn.execute(
            "INSERT INTO leases (id, task_id, agent_id, generation, acquired_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                lease.id.to_string(),
                lease.task_id.to_string(),
                lease.agent_id.to_string(),
                lease.generation,
                lease.acquired_at.to_rfc3339(),
                lease.expires_at.to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    async fn get_lease_by_task(&self, task_id: &TaskId) -> Result<Option<Lease>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, task_id, agent_id, generation, acquired_at, expires_at
             FROM leases WHERE task_id = ?1",
        )?;

        let row = stmt
            .query_row(params![task_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .optional()?;

        match row {
            Some((id_str, task_str, agent_str, generation, acq_str, exp_str)) => {
                let id: LeaseId = id_str.parse()?;
                let task_id: TaskId = task_str.parse()?;
                let agent_id: AgentId = agent_str.parse()?;
                let acquired_at = DateTime::parse_from_rfc3339(&acq_str)
                    .map_err(|e| StorageError::Migration(e.to_string()))?
                    .with_timezone(&Utc);
                let expires_at = DateTime::parse_from_rfc3339(&exp_str)
                    .map_err(|e| StorageError::Migration(e.to_string()))?
                    .with_timezone(&Utc);

                Ok(Some(Lease {
                    id,
                    task_id,
                    agent_id,
                    generation,
                    acquired_at,
                    expires_at,
                }))
            }
            None => Ok(None),
        }
    }

    async fn renew_lease(
        &self,
        lease_id: &LeaseId,
        new_expiry: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        let rows = conn.execute(
            "UPDATE leases SET expires_at = ?1 WHERE id = ?2",
            params![new_expiry.to_rfc3339(), lease_id.to_string()],
        )?;

        if rows == 0 {
            return Err(StorageError::NotFound {
                entity_type: "Lease",
                id: lease_id.to_string(),
            });
        }
        Ok(())
    }

    async fn release_lease(&self, lease_id: &LeaseId) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM leases WHERE id = ?1",
            params![lease_id.to_string()],
        )?;
        Ok(())
    }

    async fn reclaim_expired_leases(&self) -> Result<Vec<TaskId>, StorageError> {
        let conn = self.conn.lock().await;
        let now = Utc::now().to_rfc3339();

        let mut stmt = conn.prepare("SELECT task_id FROM leases WHERE expires_at <= ?1")?;

        let expired_rows = stmt.query_map(params![now], |row| {
            let s: String = row.get(0)?;
            Ok(s)
        })?;

        let mut expired_tasks = Vec::new();
        for r in expired_rows {
            let s = r?;
            expired_tasks.push(s.parse()?);
        }

        conn.execute("DELETE FROM leases WHERE expires_at <= ?1", params![now])?;

        Ok(expired_tasks)
    }
}

#[async_trait]
impl EventStore for SqliteStore {
    async fn append_event(&self, event: &Event) -> Result<(), StorageError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO events (
                id, aggregate_type, aggregate_id, event_type, payload,
                actor, causation_id, correlation_id, timestamp
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.id.to_string(),
                event.aggregate_type,
                event.aggregate_id,
                event.event_type,
                serde_json::to_string(&event.payload)?,
                event.actor,
                event.causation_id,
                event.correlation_id,
                event.timestamp.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    async fn list_events_by_aggregate(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> Result<Vec<Event>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, aggregate_type, aggregate_id, event_type, payload,
                    actor, causation_id, correlation_id, timestamp
             FROM events
             WHERE aggregate_type = ?1 AND aggregate_id = ?2
             ORDER BY timestamp ASC",
        )?;

        let rows = stmt.query_map(params![aggregate_type, aggregate_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;

        let mut events = Vec::new();
        for r in rows {
            let (id_str, agg_type, agg_id, evt_type, pay_str, actor, caus, corr, ts_str) = r?;
            let id: EventId = id_str.parse()?;
            let payload: serde_json::Value = serde_json::from_str(&pay_str)?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc);

            events.push(Event {
                id,
                aggregate_type: agg_type,
                aggregate_id: agg_id,
                event_type: evt_type,
                payload,
                actor,
                causation_id: caus,
                correlation_id: corr,
                timestamp,
            });
        }

        Ok(events)
    }

    async fn list_recent_events(&self, limit: usize) -> Result<Vec<Event>, StorageError> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, aggregate_type, aggregate_id, event_type, payload,
                    actor, causation_id, correlation_id, timestamp
             FROM events
             ORDER BY timestamp DESC
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;

        let mut events = Vec::new();
        for r in rows {
            let (id_str, agg_type, agg_id, evt_type, pay_str, actor, caus, corr, ts_str) = r?;
            let id: EventId = id_str.parse()?;
            let payload: serde_json::Value = serde_json::from_str(&pay_str)?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map_err(|e| StorageError::Migration(e.to_string()))?
                .with_timezone(&Utc);

            events.push(Event {
                id,
                aggregate_type: agg_type,
                aggregate_id: agg_id,
                event_type: evt_type,
                payload,
                actor,
                causation_id: caus,
                correlation_id: corr,
                timestamp,
            });
        }

        Ok(events)
    }
}
