//! Embedded schema migrations for SQLite.

use crate::error::StorageError;
use rusqlite::Connection;
use tracing::info;

const INITIAL_SCHEMA: &str = include_str!("../../../../migrations/0001_initial_schema.sql");
const MIGRATION_0002: &str = include_str!("../../../../migrations/0002_approvals_and_plans.sql");
const MIGRATION_0003: &str = include_str!("../../../../migrations/0003_memory_and_recovery.sql");
const MIGRATION_0004: &str =
    include_str!("../../../../migrations/0004_workspaces_and_projects.sql");

pub fn run_migrations(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;

    let applied_versions: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT version FROM _migrations ORDER BY version ASC")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    if !applied_versions.contains(&1) {
        info!("Applying migration 0001_initial_schema");
        let tx = conn.transaction()?;
        tx.execute_batch(INITIAL_SCHEMA)?;
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![1, "0001_initial_schema"],
        )?;
        tx.commit()?;
    }

    if !applied_versions.contains(&2) {
        info!("Applying migration 0002_approvals_and_plans");
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_0002)?;
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![2, "0002_approvals_and_plans"],
        )?;
        tx.commit()?;
    }

    if !applied_versions.contains(&3) {
        info!("Applying migration 0003_memory_and_recovery");
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_0003)?;
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![3, "0003_memory_and_recovery"],
        )?;
        tx.commit()?;
    }

    if !applied_versions.contains(&4) {
        info!("Applying migration 0004_workspaces_and_projects");
        let tx = conn.transaction()?;
        tx.execute_batch(MIGRATION_0004)?;
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, datetime('now'))",
            rusqlite::params![4, "0004_workspaces_and_projects"],
        )?;
        tx.commit()?;
    }

    Ok(())
}
