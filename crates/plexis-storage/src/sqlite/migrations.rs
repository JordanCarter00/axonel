//! Embedded schema migrations for SQLite.

use rusqlite::Connection;
use tracing::info;
use crate::error::StorageError;

const INITIAL_SCHEMA: &str = include_str!("../../../../migrations/0001_initial_schema.sql");

pub fn run_migrations(conn: &mut Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );"
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

    Ok(())
}
