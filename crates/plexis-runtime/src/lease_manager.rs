//! Lease management subsystem for the execution plane.

use crate::error::RuntimeError;
use chrono::{DateTime, Duration, Utc};
use plexis_core::ids::{AgentId, LeaseId, TaskId};
use plexis_core::Lease;
use plexis_storage::traits::LeaseStore;
use std::sync::Arc;

/// High-level coordinator managing lease acquisitions, fencing token checks, and renewals.
pub struct LeaseManager {
    store: Arc<dyn LeaseStore>,
}

impl LeaseManager {
    pub fn new(store: Arc<dyn LeaseStore>) -> Self {
        Self { store }
    }

    /// Acquires an execution lease for a task.
    pub async fn acquire(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        ttl: Duration,
    ) -> Result<Lease, RuntimeError> {
        let lease = Lease::new(task_id, agent_id, ttl);
        let granted = self.store.acquire_lease(&lease).await?;
        Ok(granted)
    }

    /// Validates an active lease and its generation fencing token.
    pub async fn validate_token(
        &self,
        task_id: &TaskId,
        generation: u64,
        now: DateTime<Utc>,
    ) -> Result<Lease, RuntimeError> {
        let lease = self
            .store
            .get_lease_by_task(task_id)
            .await?
            .ok_or_else(|| RuntimeError::Lease(format!("no lease found for task '{task_id}'")))?;

        lease
            .validate_token(generation, now)
            .map_err(|e| RuntimeError::Lease(e.to_string()))?;
        Ok(lease)
    }

    /// Extends an active lease.
    pub async fn renew(&self, lease_id: &LeaseId, extension: Duration) -> Result<(), RuntimeError> {
        let new_expiry = Utc::now() + extension;
        self.store.renew_lease(lease_id, new_expiry).await?;
        Ok(())
    }

    /// Releases a lease upon task completion or assignment cancellation.
    pub async fn release(&self, lease_id: &LeaseId) -> Result<(), RuntimeError> {
        self.store.release_lease(lease_id).await?;
        Ok(())
    }
}
