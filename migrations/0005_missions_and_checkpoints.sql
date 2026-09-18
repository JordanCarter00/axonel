-- Migration 0005: Long-Horizon Missions, Checkpoints, and Cycles

CREATE TABLE IF NOT EXISTS missions (
    id TEXT PRIMARY KEY,
    workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
    active_workflow_id TEXT REFERENCES workflows(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    objective TEXT NOT NULL,
    state TEXT NOT NULL,
    cycle_index INTEGER NOT NULL DEFAULT 0,
    budget TEXT NOT NULL,
    budget_consumed TEXT NOT NULL,
    health_status TEXT NOT NULL DEFAULT 'healthy',
    stopping_condition TEXT NOT NULL,
    latest_verified_commit TEXT,
    final_outcome TEXT,
    escalation_reason TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_missions_state ON missions(state);
CREATE INDEX IF NOT EXISTS idx_missions_workspace ON missions(workspace_id);

CREATE TABLE IF NOT EXISTS mission_checkpoints (
    id TEXT PRIMARY KEY,
    mission_id TEXT NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    cycle_index INTEGER NOT NULL,
    workflow_id TEXT NOT NULL,
    task_states_summary TEXT NOT NULL,
    active_executions TEXT NOT NULL DEFAULT '[]',
    completed_tasks TEXT NOT NULL DEFAULT '[]',
    unresolved_tasks TEXT NOT NULL DEFAULT '[]',
    budget_consumed TEXT NOT NULL,
    latest_verified_commit TEXT,
    planner_context TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_mission ON mission_checkpoints(mission_id, cycle_index);

CREATE TABLE IF NOT EXISTS mission_cycles (
    id TEXT PRIMARY KEY,
    mission_id TEXT NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    cycle_index INTEGER NOT NULL,
    workflow_id TEXT NOT NULL,
    phase TEXT NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    outcome TEXT,
    discovered_tasks_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_cycles_mission ON mission_cycles(mission_id, cycle_index);
