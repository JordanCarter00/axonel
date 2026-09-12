-- Migration 0003: Memory Records and Failure Recovery History

-- Persistent Memory Records (Scopes, Provenance, Embeddings, Lifecycle)
CREATE TABLE IF NOT EXISTS memory_records (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL,
    scope_id TEXT,
    source TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding_json TEXT,
    importance REAL NOT NULL DEFAULT 0.5,
    metadata_json TEXT NOT NULL,
    provenance_json TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'active',
    superseded_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    accessed_at TEXT,
    access_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_memory_scope ON memory_records(scope, scope_id);
CREATE INDEX IF NOT EXISTS idx_memory_state ON memory_records(state);
CREATE INDEX IF NOT EXISTS idx_memory_created_at ON memory_records(created_at);

-- Failure Recovery History & Strategy Audit
CREATE TABLE IF NOT EXISTS recovery_records (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    workflow_id TEXT NOT NULL,
    execution_id TEXT,
    attempt INTEGER NOT NULL,
    strategy TEXT NOT NULL,
    strategy_version INTEGER NOT NULL DEFAULT 1,
    failure_reason TEXT NOT NULL,
    diagnosis_json TEXT NOT NULL,
    recovery_action TEXT NOT NULL,
    action_reason TEXT,
    result TEXT NOT NULL DEFAULT 'in_progress',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE,
    FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_recovery_task ON recovery_records(task_id);
CREATE INDEX IF NOT EXISTS idx_recovery_workflow ON recovery_records(workflow_id);
CREATE INDEX IF NOT EXISTS idx_recovery_strategy ON recovery_records(strategy);
