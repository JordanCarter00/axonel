-- 0002_approvals_and_plans.sql
-- Human approval gates and persistent planner records for Plexis

CREATE TABLE IF NOT EXISTS approvals (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    requested_by TEXT,
    action_description TEXT NOT NULL,
    state TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL,
    decided_at TEXT,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_approvals_task ON approvals(task_id);
CREATE INDEX IF NOT EXISTS idx_approvals_workflow ON approvals(workflow_id);
CREATE INDEX IF NOT EXISTS idx_approvals_state ON approvals(state);

CREATE TABLE IF NOT EXISTS planning_records (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL,
    parent_task_id TEXT,
    objective TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    proposal TEXT NOT NULL,
    status TEXT NOT NULL,
    validation_errors TEXT,
    applied_mutations TEXT,
    latency_ms INTEGER NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_task_id) REFERENCES tasks(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_planning_records_workflow ON planning_records(workflow_id);
CREATE INDEX IF NOT EXISTS idx_planning_records_parent_task ON planning_records(parent_task_id);
