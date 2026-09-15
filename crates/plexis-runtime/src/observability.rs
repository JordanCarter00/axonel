//! Structured run observability: provenance audit trail and markdown run summary.
//!
//! Reconstructs a full audit picture from durable store data: events, tasks,
//! executions, recovery records, verifications, and approval records.  Emits
//! a human-readable markdown run report with a deterministic integrity digest.

use std::sync::Arc;

use plexis_core::{
    ApprovalRecord, ApprovalState, Event, Execution, ExecutionState, RecoveryRecord, Task,
    TaskState, Verification, VerificationVerdict, Workflow, WorkflowId,
};
use plexis_storage::traits::{
    ApprovalStore, EventStore, ExecutionStore, RecoveryStore, TaskStore, VerificationStore,
    WorkflowStore,
};

/// Severity classification of an audit event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuditSeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for AuditSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "INFO"),
            Self::Warning => write!(f, "WARN"),
            Self::Critical => write!(f, "CRIT"),
        }
    }
}

/// A single reconstructed audit event with full provenance.
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub severity: AuditSeverity,
    pub source: String,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub detail: String,
}

/// Full provenance audit trail for a workflow run.
#[derive(Debug, Default)]
pub struct ProvenanceAuditTrail {
    pub workflow_id: String,
    pub events: Vec<AuditEvent>,
}

impl ProvenanceAuditTrail {
    pub fn new(workflow_id: impl Into<String>) -> Self {
        Self {
            workflow_id: workflow_id.into(),
            events: Vec::new(),
        }
    }

    pub fn push(&mut self, event: AuditEvent) {
        self.events.push(event);
    }

    /// Returns all events at or above the given severity.
    pub fn filter_by_severity(&self, min: &AuditSeverity) -> Vec<&AuditEvent> {
        self.events.iter().filter(|e| &e.severity >= min).collect()
    }

    /// Count events whose action string contains `action`.
    pub fn count_action(&self, action: &str) -> usize {
        self.events
            .iter()
            .filter(|e| e.action.contains(action))
            .count()
    }
}

/// Aggregated statistics for a workflow run.
#[derive(Debug, Default)]
pub struct RunStats {
    pub total_tasks: usize,
    pub verified_tasks: usize,
    pub failed_tasks: usize,
    pub needs_human_tasks: usize,
    pub total_executions: usize,
    pub successful_executions: usize,
    pub failed_executions: usize,
    pub total_recoveries: usize,
    pub total_verifications: usize,
    pub passed_verifications: usize,
    pub human_approvals: usize,
    pub total_events: usize,
}

/// Security boundary assessment for the run.
#[derive(Debug, Default)]
pub struct SecurityBoundaryAudit {
    pub sandbox_violations: Vec<String>,
    pub credential_exposures: Vec<String>,
    pub path_escape_attempts: Vec<String>,
    pub privilege_escalations: Vec<String>,
}

impl SecurityBoundaryAudit {
    /// Returns true if no security violations were detected.
    pub fn is_clean(&self) -> bool {
        self.sandbox_violations.is_empty()
            && self.credential_exposures.is_empty()
            && self.path_escape_attempts.is_empty()
            && self.privilege_escalations.is_empty()
    }

    /// Summary line for embedding in the markdown report.
    pub fn summary_line(&self) -> String {
        if self.is_clean() {
            "✅ No security boundary violations detected".to_string()
        } else {
            format!(
                "⛔ {} violation(s): sandbox={}, credentials={}, path_escape={}, privilege_escalation={}",
                self.sandbox_violations.len()
                    + self.credential_exposures.len()
                    + self.path_escape_attempts.len()
                    + self.privilege_escalations.len(),
                self.sandbox_violations.len(),
                self.credential_exposures.len(),
                self.path_escape_attempts.len(),
                self.privilege_escalations.len(),
            )
        }
    }
}

/// Builds a structured `RunSummary` from stored run data.
pub struct RunSummaryBuilder<S> {
    store: Arc<S>,
}

impl<S> RunSummaryBuilder<S>
where
    S: WorkflowStore
        + TaskStore
        + ExecutionStore
        + EventStore
        + RecoveryStore
        + VerificationStore
        + ApprovalStore
        + Send
        + Sync,
{
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// Reconstructs the full audit trail + run summary for a workflow run.
    pub async fn build(
        &self,
        workflow_id: &WorkflowId,
    ) -> Result<RunSummary, plexis_storage::StorageError> {
        let workflow = self.store.get_workflow(workflow_id).await?.ok_or_else(|| {
            plexis_storage::StorageError::NotFound {
                entity_type: "Workflow",
                id: workflow_id.to_string(),
            }
        })?;

        let tasks = self.store.list_tasks_by_workflow(workflow_id).await?;
        let workflow_events = self
            .store
            .list_events_by_aggregate("workflow", &workflow_id.to_string())
            .await?;

        let mut all_executions: Vec<Execution> = Vec::new();
        let mut all_recoveries: Vec<RecoveryRecord> = Vec::new();
        let mut all_verifications: Vec<Verification> = Vec::new();
        let mut task_events: Vec<Event> = Vec::new();

        for task in &tasks {
            let execs = self.store.list_executions_by_task(&task.id).await?;
            all_executions.extend(execs);

            let recoveries = self.store.list_recovery_records_by_task(&task.id).await?;
            all_recoveries.extend(recoveries);

            let verifs = self.store.list_verifications_by_task(&task.id).await?;
            all_verifications.extend(verifs);

            let tevts = self
                .store
                .list_events_by_aggregate("task", &task.id.to_string())
                .await?;
            task_events.extend(tevts);
        }

        // Collect approvals for all tasks
        let mut all_approvals: Vec<ApprovalRecord> = Vec::new();
        for task in &tasks {
            let approvals = self
                .store
                .list_approvals_by_task(&task.id)
                .await
                .unwrap_or_default();
            all_approvals.extend(approvals);
        }

        // Build provenance trail
        let mut trail = ProvenanceAuditTrail::new(workflow_id.to_string());

        // Workflow-level events
        for evt in &workflow_events {
            trail.push(AuditEvent {
                timestamp: evt.timestamp,
                severity: AuditSeverity::Info,
                source: "EventStore".to_string(),
                entity_type: "workflow".to_string(),
                entity_id: workflow_id.to_string(),
                action: evt.event_type.clone(),
                detail: evt.payload.to_string(),
            });
        }

        // Task-level events
        for evt in &task_events {
            trail.push(AuditEvent {
                timestamp: evt.timestamp,
                severity: AuditSeverity::Info,
                source: "EventStore".to_string(),
                entity_type: "task".to_string(),
                entity_id: evt.aggregate_id.clone(),
                action: evt.event_type.clone(),
                detail: evt.payload.to_string(),
            });
        }

        // Execution-level
        for exec in &all_executions {
            let severity = if exec.state == ExecutionState::Failed {
                AuditSeverity::Warning
            } else {
                AuditSeverity::Info
            };
            trail.push(AuditEvent {
                timestamp: exec.created_at,
                severity,
                source: "ExecutionStore".to_string(),
                entity_type: "execution".to_string(),
                entity_id: exec.id.to_string(),
                action: format!("execution.{}", exec.state.as_str()),
                detail: exec
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "ok".to_string()),
            });
        }

        // Recovery events
        for rec in &all_recoveries {
            trail.push(AuditEvent {
                timestamp: rec.created_at,
                severity: AuditSeverity::Warning,
                source: "RecoveryStore".to_string(),
                entity_type: "recovery".to_string(),
                entity_id: rec.id.to_string(),
                action: "recovery.applied".to_string(),
                detail: format!(
                    "strategy={} action={} reason={}",
                    rec.strategy,
                    rec.recovery_action,
                    rec.action_reason.as_deref().unwrap_or("none")
                ),
            });
        }

        // Verification events
        for verif in &all_verifications {
            let passed = verif.verdict == VerificationVerdict::Passed;
            let severity = if passed {
                AuditSeverity::Info
            } else {
                AuditSeverity::Warning
            };
            trail.push(AuditEvent {
                timestamp: verif.created_at,
                severity,
                source: "VerificationStore".to_string(),
                entity_type: "verification".to_string(),
                entity_id: verif.id.to_string(),
                action: if passed {
                    "verification.passed".to_string()
                } else {
                    "verification.failed".to_string()
                },
                detail: verif
                    .failure_reason
                    .as_deref()
                    .unwrap_or("no failure reason")
                    .to_string(),
            });
        }

        // Approval events
        for approval in &all_approvals {
            trail.push(AuditEvent {
                timestamp: approval.created_at,
                severity: AuditSeverity::Info,
                source: "ApprovalStore".to_string(),
                entity_type: "approval".to_string(),
                entity_id: approval.id.to_string(),
                action: format!("approval.{}", approval.state.as_str()),
                detail: format!(
                    "action={} reason={}",
                    approval.action_description,
                    approval.reason.as_deref().unwrap_or("none")
                ),
            });
        }

        // Sort audit trail by timestamp
        trail.events.sort_by_key(|e| e.timestamp);

        // Security boundary scan: look for suspicious patterns in event payloads
        let mut security = SecurityBoundaryAudit::default();
        for evt in trail.events.iter() {
            let detail_lower = evt.detail.to_lowercase();
            if detail_lower.contains("../") || detail_lower.contains("..\\") {
                security
                    .path_escape_attempts
                    .push(format!("[{}] {}", evt.entity_id, evt.detail));
            }
            if detail_lower.contains("api_key") || detail_lower.contains("token=") {
                security
                    .credential_exposures
                    .push(format!("[{}] {}", evt.entity_id, evt.action));
            }
            if detail_lower.contains("sudo") || detail_lower.contains("privilege") {
                security
                    .privilege_escalations
                    .push(format!("[{}] {}", evt.entity_id, evt.detail));
            }
        }

        // Aggregate stats
        let stats = RunStats {
            total_tasks: tasks.len(),
            verified_tasks: tasks
                .iter()
                .filter(|t| t.state == TaskState::Verified)
                .count(),
            failed_tasks: tasks
                .iter()
                .filter(|t| t.state == TaskState::Failed)
                .count(),
            needs_human_tasks: tasks
                .iter()
                .filter(|t| t.state == TaskState::NeedsHuman)
                .count(),
            total_executions: all_executions.len(),
            successful_executions: all_executions
                .iter()
                .filter(|e| e.state == ExecutionState::Completed)
                .count(),
            failed_executions: all_executions
                .iter()
                .filter(|e| e.state == ExecutionState::Failed)
                .count(),
            total_recoveries: all_recoveries.len(),
            total_verifications: all_verifications.len(),
            passed_verifications: all_verifications
                .iter()
                .filter(|v| v.verdict == VerificationVerdict::Passed)
                .count(),
            human_approvals: all_approvals
                .iter()
                .filter(|a| a.state == ApprovalState::Approved)
                .count(),
            total_events: workflow_events.len() + task_events.len(),
        };

        Ok(RunSummary {
            workflow,
            tasks,
            stats,
            trail,
            security,
        })
    }
}

/// A complete structured run summary with embedded provenance audit trail.
pub struct RunSummary {
    pub workflow: Workflow,
    pub tasks: Vec<Task>,
    pub stats: RunStats,
    pub trail: ProvenanceAuditTrail,
    pub security: SecurityBoundaryAudit,
}

impl RunSummary {
    /// Renders the full markdown report.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        // Header
        md.push_str(&format!(
            "# Plexis Run Summary — `{}`\n\n",
            self.workflow.id
        ));
        md.push_str(&format!(
            "> **Objective:** {}\n>\n",
            self.workflow.objective
        ));
        md.push_str(&format!(
            "> **Workflow State:** `{:?}`\n>\n",
            self.workflow.state
        ));
        md.push_str(&format!(
            "> **Generated:** `{}`\n\n",
            chrono::Utc::now().to_rfc3339()
        ));
        md.push_str("---\n\n");

        // Statistics
        md.push_str("## § Run Statistics\n\n");
        md.push_str("| Metric | Value |\n|--------|-------|\n");
        md.push_str(&format!("| Total Tasks | {} |\n", self.stats.total_tasks));
        md.push_str(&format!(
            "| Verified Tasks | ✅ {} |\n",
            self.stats.verified_tasks
        ));
        md.push_str(&format!(
            "| Failed Tasks | ❌ {} |\n",
            self.stats.failed_tasks
        ));
        md.push_str(&format!(
            "| NeedsHuman Tasks | 🔔 {} |\n",
            self.stats.needs_human_tasks
        ));
        md.push_str(&format!(
            "| Total Executions | {} |\n",
            self.stats.total_executions
        ));
        md.push_str(&format!(
            "| Successful Executions | {} |\n",
            self.stats.successful_executions
        ));
        md.push_str(&format!(
            "| Failed Executions | {} |\n",
            self.stats.failed_executions
        ));
        md.push_str(&format!(
            "| Recovery Attempts | {} |\n",
            self.stats.total_recoveries
        ));
        md.push_str(&format!(
            "| Verifications | {}/{} passed |\n",
            self.stats.passed_verifications, self.stats.total_verifications
        ));
        md.push_str(&format!(
            "| Human Approvals | {} |\n",
            self.stats.human_approvals
        ));
        md.push_str(&format!("| Audit Events | {} |\n", self.stats.total_events));
        md.push('\n');

        // Task roster
        md.push_str("## § Task Roster\n\n");
        md.push_str("| # | Task | State | Role |\n|---|------|-------|------|\n");
        for (i, task) in self.tasks.iter().enumerate() {
            let icon = match &task.state {
                TaskState::Verified => "✅",
                TaskState::Failed => "❌",
                TaskState::NeedsHuman => "🔔",
                TaskState::Running => "⚡",
                TaskState::Ready => "🟡",
                TaskState::Blocked => "🔒",
                TaskState::Cancelled => "🚫",
                TaskState::Discarded => "🗑",
                TaskState::AwaitingVerification => "🔍",
                _ => "⬜",
            };
            let role = task
                .metadata
                .get("suggested_role")
                .and_then(|v| v.as_str())
                .unwrap_or("—");
            md.push_str(&format!(
                "| {} | {} {} | `{:?}` | {} |\n",
                i + 1,
                icon,
                task.objective,
                task.state,
                role
            ));
        }
        md.push('\n');

        // Security boundary audit
        md.push_str("## § Security Boundary Audit\n\n");
        md.push_str(&format!("{}\n\n", self.security.summary_line()));

        if !self.security.sandbox_violations.is_empty() {
            md.push_str("### Sandbox Violations\n");
            for v in &self.security.sandbox_violations {
                md.push_str(&format!("- `{}`\n", v));
            }
            md.push('\n');
        }
        if !self.security.path_escape_attempts.is_empty() {
            md.push_str("### Path Escape Attempts\n");
            for v in &self.security.path_escape_attempts {
                md.push_str(&format!("- `{}`\n", v));
            }
            md.push('\n');
        }
        if !self.security.credential_exposures.is_empty() {
            md.push_str("### Credential Exposure Events\n");
            for v in &self.security.credential_exposures {
                md.push_str(&format!("- `{}`\n", v));
            }
            md.push('\n');
        }

        // Provenance audit trail (last 50 events to keep report tractable)
        md.push_str("## § Provenance Audit Trail\n\n");
        md.push_str("| Timestamp | Severity | Source | Entity | Action | Detail |\n");
        md.push_str("|-----------|----------|--------|--------|--------|--------|\n");

        let events_to_show = if self.trail.events.len() > 50 {
            &self.trail.events[self.trail.events.len() - 50..]
        } else {
            &self.trail.events[..]
        };

        for evt in events_to_show {
            let ts = evt.timestamp.format("%H:%M:%S%.3f").to_string();
            let detail_short = if evt.detail.len() > 80 {
                format!("{}…", &evt.detail[..80])
            } else {
                evt.detail.clone()
            };
            let id_short = &evt.entity_id[..evt.entity_id.len().min(8)];
            md.push_str(&format!(
                "| `{}` | `{}` | `{}` | `{}/{}` | `{}` | {} |\n",
                ts, evt.severity, evt.source, evt.entity_type, id_short, evt.action, detail_short
            ));
        }

        if self.trail.events.len() > 50 {
            md.push_str(&format!(
                "\n> _Showing last 50 of {} events. Full trail available in EventStore._\n",
                self.trail.events.len()
            ));
        }
        md.push('\n');

        // Integrity digest (deterministic hash for report provenance)
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.workflow.id.to_string().hash(&mut hasher);
        self.stats.total_tasks.hash(&mut hasher);
        self.stats.total_executions.hash(&mut hasher);
        self.stats.total_recoveries.hash(&mut hasher);
        self.trail.events.len().hash(&mut hasher);
        let digest = hasher.finish();

        md.push_str("---\n\n");
        md.push_str("## § Report Integrity\n\n");
        md.push_str(&format!(
            "| Field | Value |\n|-------|-------|\n| Report Digest | `{:016x}` |\n| Audit Events | {} |\n| Workflow ID | `{}` |\n",
            digest,
            self.trail.events.len(),
            self.workflow.id,
        ));

        md
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_severity_ordering() {
        assert!(AuditSeverity::Critical > AuditSeverity::Warning);
        assert!(AuditSeverity::Warning > AuditSeverity::Info);
    }

    #[test]
    fn test_security_boundary_audit_clean() {
        let audit = SecurityBoundaryAudit::default();
        assert!(audit.is_clean());
        assert!(audit.summary_line().starts_with('✅'));
    }

    #[test]
    fn test_security_boundary_audit_violation() {
        let mut audit = SecurityBoundaryAudit::default();
        audit.path_escape_attempts.push("../etc/passwd".to_string());
        assert!(!audit.is_clean());
        assert!(audit.summary_line().starts_with('⛔'));
    }

    #[test]
    fn test_provenance_trail_filter() {
        let mut trail = ProvenanceAuditTrail::new("wf-test");
        trail.push(AuditEvent {
            timestamp: chrono::Utc::now(),
            severity: AuditSeverity::Info,
            source: "test".into(),
            entity_type: "workflow".into(),
            entity_id: "wf-1".into(),
            action: "workflow.started".into(),
            detail: "ok".into(),
        });
        trail.push(AuditEvent {
            timestamp: chrono::Utc::now(),
            severity: AuditSeverity::Warning,
            source: "test".into(),
            entity_type: "recovery".into(),
            entity_id: "rec-1".into(),
            action: "recovery.applied".into(),
            detail: "strategy=MutateStrategy".into(),
        });
        trail.push(AuditEvent {
            timestamp: chrono::Utc::now(),
            severity: AuditSeverity::Critical,
            source: "test".into(),
            entity_type: "security".into(),
            entity_id: "sec-1".into(),
            action: "sandbox.violation".into(),
            detail: "path escape attempt".into(),
        });

        let warnings_plus = trail.filter_by_severity(&AuditSeverity::Warning);
        assert_eq!(warnings_plus.len(), 2);

        let crits = trail.filter_by_severity(&AuditSeverity::Critical);
        assert_eq!(crits.len(), 1);

        assert_eq!(trail.count_action("recovery"), 1);
    }

    #[test]
    fn test_run_summary_markdown_shape() {
        let workflow = plexis_core::Workflow::new("Test run", "Test objective");

        let stats = RunStats {
            total_tasks: 3,
            verified_tasks: 2,
            failed_tasks: 1,
            ..Default::default()
        };

        let trail = ProvenanceAuditTrail::new(workflow.id.to_string());
        let security = SecurityBoundaryAudit::default();

        let summary = RunSummary {
            workflow,
            tasks: vec![],
            stats,
            trail,
            security,
        };

        let md = summary.to_markdown();
        assert!(md.contains("# Plexis Run Summary"));
        assert!(md.contains("§ Run Statistics"));
        assert!(md.contains("§ Task Roster"));
        assert!(md.contains("§ Security Boundary Audit"));
        assert!(md.contains("§ Provenance Audit Trail"));
        assert!(md.contains("§ Report Integrity"));
        assert!(md.contains("No security boundary violations detected"));
    }
}
