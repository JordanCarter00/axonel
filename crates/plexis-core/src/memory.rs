//! Memory domain models for persistent knowledge, provenance, and lifecycle.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, EventId, MemoryId, SessionId, TaskId, WorkflowId};

/// Logical scope to which a memory record belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScope {
    System,
    User,
    Project,
    Workflow,
    Task,
    Agent,
    Session,
    Artifact,
}

impl MemoryScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Project => "project",
            Self::Workflow => "workflow",
            Self::Task => "task",
            Self::Agent => "agent",
            Self::Session => "session",
            Self::Artifact => "artifact",
        }
    }
}

impl std::fmt::Display for MemoryScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for MemoryScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "project" => Ok(Self::Project),
            "workflow" => Ok(Self::Workflow),
            "task" => Ok(Self::Task),
            "agent" => Ok(Self::Agent),
            "session" => Ok(Self::Session),
            "artifact" => Ok(Self::Artifact),
            other => Err(format!("Unknown memory scope: '{}'", other)),
        }
    }
}

/// Lifecycle state of a memory record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryState {
    #[default]
    Active,
    Superseded,
    Archived,
    Deleted,
}

impl MemoryState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Archived => "archived",
            Self::Deleted => "deleted",
        }
    }
}

impl std::fmt::Display for MemoryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for MemoryState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            "archived" => Ok(Self::Archived),
            "deleted" => Ok(Self::Deleted),
            other => Err(format!("Unknown memory state: '{}'", other)),
        }
    }
}

/// Provenance trace explaining where a memory originated and why it was recalled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProvenance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub originating_task_id: Option<TaskId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub originating_workflow_id: Option<WorkflowId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub originating_session_id: Option<SessionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub originating_agent_id: Option<AgentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<EventId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_reason: Option<String>,
}

impl MemoryProvenance {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_task(mut self, task_id: TaskId) -> Self {
        self.originating_task_id = Some(task_id);
        self
    }

    pub fn with_workflow(mut self, workflow_id: WorkflowId) -> Self {
        self.originating_workflow_id = Some(workflow_id);
        self
    }

    pub fn with_session(mut self, session_id: SessionId) -> Self {
        self.originating_session_id = Some(session_id);
        self
    }

    pub fn with_agent(mut self, agent_id: AgentId) -> Self {
        self.originating_agent_id = Some(agent_id);
        self
    }

    pub fn with_retrieval_reason(mut self, reason: impl Into<String>) -> Self {
        self.retrieval_reason = Some(reason.into());
        self
    }
}

/// Durable persistent memory record representing long-term knowledge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: MemoryId,
    pub scope: MemoryScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    pub source: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub importance: f32,
    pub metadata: serde_json::Value,
    pub provenance: MemoryProvenance,
    pub state: MemoryState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<MemoryId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed_at: Option<DateTime<Utc>>,
    pub access_count: u32,
}

impl MemoryRecord {
    pub fn new(scope: MemoryScope, source: impl Into<String>, content: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: MemoryId::new(),
            scope,
            scope_id: None,
            source: source.into(),
            content: content.into(),
            embedding: None,
            importance: 0.5,
            metadata: serde_json::Value::Object(Default::default()),
            provenance: MemoryProvenance::default(),
            state: MemoryState::Active,
            superseded_by: None,
            created_at: now,
            updated_at: now,
            accessed_at: None,
            access_count: 0,
        }
    }

    pub fn with_scope_id(mut self, scope_id: impl Into<String>) -> Self {
        self.scope_id = Some(scope_id.into());
        self
    }

    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_provenance(mut self, provenance: MemoryProvenance) -> Self {
        self.provenance = provenance;
        self
    }

    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    pub fn mark_accessed(&mut self) {
        self.accessed_at = Some(Utc::now());
        self.access_count += 1;
    }

    pub fn supersede_with(&mut self, new_memory_id: MemoryId) {
        self.state = MemoryState::Superseded;
        self.superseded_by = Some(new_memory_id);
        self.updated_at = Utc::now();
    }

    pub fn archive(&mut self) {
        self.state = MemoryState::Archived;
        self.updated_at = Utc::now();
    }

    pub fn soft_delete(&mut self) {
        self.state = MemoryState::Deleted;
        self.updated_at = Utc::now();
    }
}
