//! Mission-to-workflow execution orchestration interface.
//!
//! Provides the execution bridge between the high-level `MissionEngine`
//! and the underlying `Workflow`/`TaskGraph`/`DeterministicScheduler`
//! without duplicating or bypassing existing task execution components.

use async_trait::async_trait;
use plexis_core::ids::{ExecutionId, MissionId, WorkflowId};
use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;

/// Summary report returned after executing a workflow cycle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowExecutionSummary {
    /// Number of tasks actually executed by agents during this cycle.
    pub executed_tasks_count: usize,
    /// Total number of tasks verified / completed.
    pub completed_tasks_count: usize,
    /// Total number of tasks that encountered failure.
    pub failed_tasks_count: usize,
    /// Number of dynamically discovered tasks appended to the DAG.
    pub discovered_tasks_count: usize,
    /// In-flight execution IDs across the process host.
    pub active_executions: Vec<ExecutionId>,
    /// Additional diagnostic details or logs.
    pub details: serde_json::Value,
}

/// Orchestrates execution of the active workflow DAG for an autonomous mission.
#[async_trait]
pub trait WorkflowExecutor: Send + Sync {
    /// Executes tasks in the active workflow until batch boundary or state change.
    async fn execute_workflow_cycle(
        &self,
        workflow_id: &WorkflowId,
        mission_id: &MissionId,
    ) -> Result<WorkflowExecutionSummary, RuntimeError>;
}
