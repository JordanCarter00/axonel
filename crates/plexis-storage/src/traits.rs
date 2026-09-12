//! Abstract repository interfaces for Plexis storage.
//!
//! Domain logic depends on these traits rather than concrete database drivers.

use async_trait::async_trait;
use plexis_core::ids::{AgentId, CommandId, LeaseId, TaskId, WorkflowId};
use plexis_core::{Agent, Command, Event, Lease, Task, TaskGraph, Workflow};

use crate::error::StorageError;

/// Repository for Workflow entities.
#[async_trait]
pub trait WorkflowStore: Send + Sync {
    async fn create_workflow(&self, workflow: &Workflow) -> Result<(), StorageError>;
    async fn get_workflow(&self, id: &WorkflowId) -> Result<Option<Workflow>, StorageError>;
    async fn update_workflow(&self, workflow: &Workflow) -> Result<(), StorageError>;
    async fn list_workflows(&self) -> Result<Vec<Workflow>, StorageError>;
}

/// Repository for Task entities and dependency edges.
#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn create_task(&self, task: &Task) -> Result<(), StorageError>;
    async fn get_task(&self, id: &TaskId) -> Result<Option<Task>, StorageError>;
    async fn update_task(&self, task: &Task) -> Result<(), StorageError>;
    async fn list_tasks_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<Vec<Task>, StorageError>;
    async fn add_dependency(
        &self,
        task_id: &TaskId,
        depends_on_id: &TaskId,
    ) -> Result<(), StorageError>;
    async fn get_dependencies(&self, task_id: &TaskId) -> Result<Vec<TaskId>, StorageError>;
    async fn get_dependents(&self, task_id: &TaskId) -> Result<Vec<TaskId>, StorageError>;
    async fn load_task_graph(&self, workflow_id: &WorkflowId) -> Result<TaskGraph, StorageError>;
}

/// Repository for Agent entities.
#[async_trait]
pub trait AgentStore: Send + Sync {
    async fn create_agent(&self, agent: &Agent) -> Result<(), StorageError>;
    async fn get_agent(&self, id: &AgentId) -> Result<Option<Agent>, StorageError>;
    async fn update_agent(&self, agent: &Agent) -> Result<(), StorageError>;
    async fn list_agents(&self) -> Result<Vec<Agent>, StorageError>;
}

/// Repository for durable Commands.
#[async_trait]
pub trait CommandStore: Send + Sync {
    async fn enqueue_command(&self, command: &Command) -> Result<(), StorageError>;
    async fn get_command(&self, id: &CommandId) -> Result<Option<Command>, StorageError>;
    async fn get_by_idempotency_key(&self, key: &str) -> Result<Option<Command>, StorageError>;
    async fn claim_next_queued_command(&self) -> Result<Option<Command>, StorageError>;
    async fn update_command(&self, command: &Command) -> Result<(), StorageError>;
}

/// Repository for durable Leases.
#[async_trait]
pub trait LeaseStore: Send + Sync {
    async fn acquire_lease(&self, lease: &Lease) -> Result<(), StorageError>;
    async fn get_lease_by_task(&self, task_id: &TaskId) -> Result<Option<Lease>, StorageError>;
    async fn renew_lease(
        &self,
        lease_id: &LeaseId,
        new_expiry: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), StorageError>;
    async fn release_lease(&self, lease_id: &LeaseId) -> Result<(), StorageError>;
    async fn reclaim_expired_leases(&self) -> Result<Vec<TaskId>, StorageError>;
}

/// Repository for immutable audit Events.
#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append_event(&self, event: &Event) -> Result<(), StorageError>;
    async fn list_events_by_aggregate(
        &self,
        aggregate_type: &str,
        aggregate_id: &str,
    ) -> Result<Vec<Event>, StorageError>;
    async fn list_recent_events(&self, limit: usize) -> Result<Vec<Event>, StorageError>;
}
