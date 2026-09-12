//! Abstract repository interfaces for Plexis storage.
//!
//! Domain logic depends on these traits rather than concrete database drivers.

use async_trait::async_trait;
use plexis_core::ids::{
    AgentId, ApprovalId, CommandId, ExecutionId, LeaseId, MemoryId, MessageId, PlanId, RecoveryId,
    SessionId, TaskId, VerificationId, WorkflowId,
};
use plexis_core::{
    Agent, AgentMessage, ApprovalRecord, Command, Event, Execution, Lease, MemoryRecord,
    MemoryScope, PlanningRecord, RecoveryRecord, Session, Task, TaskGraph, Verification, Workflow,
};

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
    async fn remove_dependency(
        &self,
        task_id: &TaskId,
        depends_on_id: &TaskId,
    ) -> Result<(), StorageError>;
    async fn get_dependencies(&self, task_id: &TaskId) -> Result<Vec<TaskId>, StorageError>;
    async fn get_dependents(&self, task_id: &TaskId) -> Result<Vec<TaskId>, StorageError>;
    async fn load_task_graph(&self, workflow_id: &WorkflowId) -> Result<TaskGraph, StorageError>;
    async fn get_current_lease_generation(&self, task_id: &TaskId) -> Result<u64, StorageError>;
    async fn increment_lease_generation(&self, task_id: &TaskId) -> Result<u64, StorageError>;
    async fn decompose_task_transactional(
        &self,
        parent_id: &TaskId,
        children: &[Task],
        child_dependencies: &[(TaskId, TaskId)],
        terminal_child_ids: &[TaskId],
    ) -> Result<(), StorageError>;
}

/// Repository for Agent entities.
#[async_trait]
pub trait AgentStore: Send + Sync {
    async fn create_agent(&self, agent: &Agent) -> Result<(), StorageError>;
    async fn get_agent(&self, id: &AgentId) -> Result<Option<Agent>, StorageError>;
    async fn update_agent(&self, agent: &Agent) -> Result<(), StorageError>;
    async fn list_agents(&self) -> Result<Vec<Agent>, StorageError>;
}

/// Repository for persistent agent Sessions.
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create_session(&self, session: &Session) -> Result<(), StorageError>;
    async fn get_session(&self, id: &SessionId) -> Result<Option<Session>, StorageError>;
    async fn update_session(&self, session: &Session) -> Result<(), StorageError>;
    async fn list_sessions_by_agent(
        &self,
        agent_id: &AgentId,
    ) -> Result<Vec<Session>, StorageError>;
}

/// Repository for concrete Task Executions.
#[async_trait]
pub trait ExecutionStore: Send + Sync {
    async fn create_execution(&self, execution: &Execution) -> Result<(), StorageError>;
    async fn get_execution(&self, id: &ExecutionId) -> Result<Option<Execution>, StorageError>;
    async fn update_execution(&self, execution: &Execution) -> Result<(), StorageError>;
    async fn list_executions_by_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<Execution>, StorageError>;
    async fn list_executions_by_agent(
        &self,
        agent_id: &AgentId,
    ) -> Result<Vec<Execution>, StorageError>;
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
    async fn acquire_lease(&self, lease: &Lease) -> Result<Lease, StorageError>;
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
    async fn list_events_after(
        &self,
        after_sequence: u64,
        limit: usize,
    ) -> Result<Vec<Event>, StorageError>;
    async fn get_latest_event_sequence(&self) -> Result<u64, StorageError>;
}

/// Repository for independent Verification records.
#[async_trait]
pub trait VerificationStore: Send + Sync {
    async fn create_verification(&self, verification: &Verification) -> Result<(), StorageError>;
    async fn get_verification(
        &self,
        id: &VerificationId,
    ) -> Result<Option<Verification>, StorageError>;
    async fn list_verifications_by_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<Verification>, StorageError>;
}

/// Repository for durable Agent-to-Agent Messages.
#[async_trait]
pub trait MessageStore: Send + Sync {
    async fn send_message(&self, message: &AgentMessage) -> Result<(), StorageError>;
    async fn get_message(&self, id: &MessageId) -> Result<Option<AgentMessage>, StorageError>;
    async fn list_messages_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<Vec<AgentMessage>, StorageError>;
    async fn list_messages_by_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<AgentMessage>, StorageError>;
    async fn list_messages_for_agent(
        &self,
        agent_id: &AgentId,
    ) -> Result<Vec<AgentMessage>, StorageError>;
}

/// Repository for durable human Approval records.
#[async_trait]
pub trait ApprovalStore: Send + Sync {
    async fn create_approval(&self, approval: &ApprovalRecord) -> Result<(), StorageError>;
    async fn get_approval(&self, id: &ApprovalId) -> Result<Option<ApprovalRecord>, StorageError>;
    async fn update_approval(&self, approval: &ApprovalRecord) -> Result<(), StorageError>;
    async fn list_approvals_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<Vec<ApprovalRecord>, StorageError>;
    async fn list_approvals_by_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<ApprovalRecord>, StorageError>;
}

/// Repository for persistent Planning runs.
#[async_trait]
pub trait PlanStore: Send + Sync {
    async fn save_plan_record(&self, record: &PlanningRecord) -> Result<(), StorageError>;
    async fn get_plan_record(&self, id: &PlanId) -> Result<Option<PlanningRecord>, StorageError>;
    async fn list_plan_records_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<Vec<PlanningRecord>, StorageError>;
}

/// Repository for persistent long-term Memory records.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn save_memory(&self, memory: &MemoryRecord) -> Result<(), StorageError>;
    async fn get_memory(&self, id: &MemoryId) -> Result<Option<MemoryRecord>, StorageError>;
    async fn update_memory(&self, memory: &MemoryRecord) -> Result<(), StorageError>;
    async fn list_memories_by_scope(
        &self,
        scope: MemoryScope,
        scope_id: Option<&str>,
    ) -> Result<Vec<MemoryRecord>, StorageError>;
    async fn list_active_memories(
        &self,
        scopes: Option<&[MemoryScope]>,
        scope_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryRecord>, StorageError>;
}

/// Repository for durable failure Recovery records and strategy audits.
#[async_trait]
pub trait RecoveryStore: Send + Sync {
    async fn save_recovery_record(&self, record: &RecoveryRecord) -> Result<(), StorageError>;
    async fn get_recovery_record(
        &self,
        id: &RecoveryId,
    ) -> Result<Option<RecoveryRecord>, StorageError>;
    async fn update_recovery_record(&self, record: &RecoveryRecord) -> Result<(), StorageError>;
    async fn list_recovery_records_by_task(
        &self,
        task_id: &TaskId,
    ) -> Result<Vec<RecoveryRecord>, StorageError>;
    async fn list_recovery_records_by_workflow(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<Vec<RecoveryRecord>, StorageError>;
}
