//! Durable Lease concurrency model for Plexis.
//!
//! Assignments use leases with monotonic generation counters. This guarantees
//! mutual exclusion for task execution and prevents stale or partitioned workers
//! from overwriting newer assignments (fencing token pattern).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, LeaseId, TaskId};

/// Error related to lease operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LeaseError {
    #[error("lease expired at {expired_at}, current time is {now}")]
    Expired {
        expired_at: DateTime<Utc>,
        now: DateTime<Utc>,
    },
    #[error("lease generation mismatch: expected {expected}, actual {actual}")]
    GenerationMismatch { expected: u64, actual: u64 },
    #[error("lease held by different agent: held by {holder}, requested by {requester}")]
    AgentMismatch {
        holder: AgentId,
        requester: AgentId,
    },
}

/// Durable lease granting exclusive right to execute a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    /// Unique lease identifier.
    pub id: LeaseId,
    /// Target task guarded by this lease.
    pub task_id: TaskId,
    /// Agent holding the lease.
    pub agent_id: AgentId,
    /// Monotonic fencing token counter.
    pub generation: u64,
    /// Timestamp when lease was acquired.
    pub acquired_at: DateTime<Utc>,
    /// Timestamp when lease will expire if not renewed.
    pub expires_at: DateTime<Utc>,
}

impl Lease {
    /// Creates a new lease with initial generation = 1.
    pub fn new(task_id: TaskId, agent_id: AgentId, ttl: Duration) -> Self {
        let now = Utc::now();
        Self {
            id: LeaseId::new(),
            task_id,
            agent_id,
            generation: 1,
            acquired_at: now,
            expires_at: now + ttl,
        }
    }

    /// Checks if the lease has expired at time `now`.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Renews the lease duration without incrementing generation.
    pub fn renew(&mut self, extension: Duration, now: DateTime<Utc>) -> Result<(), LeaseError> {
        if self.is_expired(now) {
            return Err(LeaseError::Expired {
                expired_at: self.expires_at,
                now,
            });
        }
        self.expires_at = now + extension;
        Ok(())
    }

    /// Reassigns the lease to an agent (possibly new), incrementing the generation counter.
    pub fn reassign(&mut self, new_agent_id: AgentId, ttl: Duration, now: DateTime<Utc>) {
        self.agent_id = new_agent_id;
        self.generation += 1;
        self.acquired_at = now;
        self.expires_at = now + ttl;
    }

    /// Validates that an incoming operation carries the matching generation fencing token.
    pub fn validate_token(&self, generation: u64, now: DateTime<Utc>) -> Result<(), LeaseError> {
        if self.generation != generation {
            return Err(LeaseError::GenerationMismatch {
                expected: self.generation,
                actual: generation,
            });
        }
        if self.is_expired(now) {
            return Err(LeaseError::Expired {
                expired_at: self.expires_at,
                now,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lease_expiry_and_validation() {
        let task_id = TaskId::new();
        let agent_id = AgentId::new();
        let lease = Lease::new(task_id, agent_id, Duration::seconds(30));

        assert!(!lease.is_expired(Utc::now()));
        assert!(lease.validate_token(1, Utc::now()).is_ok());
        assert!(lease.validate_token(2, Utc::now()).is_err());

        let future = Utc::now() + Duration::seconds(35);
        assert!(lease.is_expired(future));
        assert!(lease.validate_token(1, future).is_err());
    }

    #[test]
    fn test_lease_reassign_generation_increment() {
        let task_id = TaskId::new();
        let agent_1 = AgentId::new();
        let agent_2 = AgentId::new();
        let mut lease = Lease::new(task_id, agent_1, Duration::seconds(30));
        assert_eq!(lease.generation, 1);

        lease.reassign(agent_2, Duration::seconds(30), Utc::now());
        assert_eq!(lease.generation, 2);
        assert_eq!(lease.agent_id, agent_2);
    }
}
