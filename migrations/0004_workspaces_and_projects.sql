-- Migration 0004: Workspaces and Project Context

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    canonical_path TEXT NOT NULL UNIQUE,
    policy TEXT NOT NULL,
    limits TEXT NOT NULL,
    vcs TEXT NOT NULL,
    metadata TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_workspaces_canonical_path ON workspaces(canonical_path);

-- Alter workflows and tasks to associate with workspaces
ALTER TABLE workflows ADD COLUMN workspace_id TEXT;
ALTER TABLE tasks ADD COLUMN workspace_id TEXT;

CREATE INDEX IF NOT EXISTS idx_workflows_workspace_id ON workflows(workspace_id);
CREATE INDEX IF NOT EXISTS idx_tasks_workspace_id ON tasks(workspace_id);
