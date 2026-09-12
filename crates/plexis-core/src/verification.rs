//! Independent Verification domain model for Plexis.
//!
//! Verification is strictly independent of the agent performing the work.
//! Tasks must produce durable verification evidence before achieving `Verified` state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{TaskId, VerificationId};

/// Verdict of an independent verification run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationVerdict {
    /// Verification passed all acceptance criteria.
    Passed,
    /// Verification detected explicit failures or regressions.
    Failed,
    /// Verifier could not reach a definitive conclusion (needs retry/human).
    Inconclusive,
}

/// Durable record of an independent verification check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// Unique verification record identifier.
    pub id: VerificationId,
    /// Target task verified.
    pub task_id: TaskId,
    /// Kind of verifier (e.g. "test_suite", "type_checker", "human_review", "linter").
    pub verifier_kind: String,
    /// Final verdict.
    pub verdict: VerificationVerdict,
    /// Structured evidence produced by verifier (logs, exit codes, assertions).
    pub evidence: serde_json::Value,
    /// Failure reason if verdict is not Passed.
    pub failure_reason: Option<String>,
    /// Execution duration of verification in milliseconds.
    pub duration_ms: u64,
    /// Timestamp when verification was conducted.
    pub created_at: DateTime<Utc>,
}

impl Verification {
    pub fn new(
        task_id: TaskId,
        verifier_kind: impl Into<String>,
        verdict: VerificationVerdict,
        evidence: serde_json::Value,
        duration_ms: u64,
    ) -> Self {
        Self {
            id: VerificationId::new(),
            task_id,
            verifier_kind: verifier_kind.into(),
            verdict,
            evidence,
            failure_reason: None,
            duration_ms,
            created_at: Utc::now(),
        }
    }

    pub fn with_failure_reason(mut self, reason: impl Into<String>) -> Self {
        self.failure_reason = Some(reason.into());
        self
    }
}
