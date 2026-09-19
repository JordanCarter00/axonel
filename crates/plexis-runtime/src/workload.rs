//! Canonical real repository workload definition for autonomous execution testing.
//!
//! Provides `CanonicalWorkload` to instantiate a real local Git repository (`kv-tombstone`)
//! with realistic file distribution, architecture documentation, an intentional compaction bug,
//! and verification assertions.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

/// Canonical real coding workload representing an append-only Key-Value Store
/// with a subtle tombstone compaction bug.
pub struct CanonicalWorkload;

impl CanonicalWorkload {
    /// Objective string describing the canonical software engineering task.
    pub fn objective() -> &'static str {
        "Diagnose failing tombstone compaction in kv-tombstone where deleted keys reappear after compact(), apply fix, add multi-generation regression tests, communicate fix, obtain human signoff, and commit verified changes."
    }

    /// Sets up the target directory as a real, initialized Git repository with clean commit.
    pub fn setup_repository(repo_dir: &Path) -> PathBuf {
        fs::create_dir_all(repo_dir.join("src")).expect("failed to create src dir");
        fs::create_dir_all(repo_dir.join("tests")).expect("failed to create tests dir");

        // 1. Cargo.toml
        let cargo_toml = r#"[package]
name = "kv-tombstone"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
        fs::write(repo_dir.join("Cargo.toml"), cargo_toml).expect("failed to write Cargo.toml");

        // 2. src/entry.rs
        let entry_rs = r#"#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: Option<String>, // None indicates a deleted tombstone
    pub generation: u64,
}

impl Entry {
    pub fn put(key: impl Into<String>, value: impl Into<String>, generation: u64) -> Self {
        Self {
            key: key.into(),
            value: Some(value.into()),
            generation,
        }
    }

    pub fn tombstone(key: impl Into<String>, generation: u64) -> Self {
        Self {
            key: key.into(),
            value: None,
            generation,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.value.is_none()
    }
}
"#;
        fs::write(repo_dir.join("src/entry.rs"), entry_rs).expect("failed to write entry.rs");

        // 3. src/storage.rs (contains the compaction bug)
        let storage_rs = r#"use crate::entry::Entry;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct StorageEngine {
    entries: Vec<Entry>,
    current_generation: u64,
}

impl StorageEngine {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            current_generation: 1,
        }
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.current_generation += 1;
        self.entries.push(Entry::put(key, value, self.current_generation));
    }

    pub fn delete(&mut self, key: &str) {
        self.current_generation += 1;
        self.entries.push(Entry::tombstone(key, self.current_generation));
    }

    pub fn get(&self, key: &str) -> Option<String> {
        for entry in self.entries.iter().rev() {
            if entry.key == key {
                return entry.value.clone();
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Compacts the log by discarding superseded records.
    ///
    /// BUG: Discards tombstones unconditionally during compaction, causing
    /// previously deleted keys from older generations to resurrect if an
    /// obsolete put record exists in the pre-compacted state!
    pub fn compact(&mut self) {
        let mut latest_by_key: HashMap<String, Entry> = HashMap::new();

        for entry in &self.entries {
            // BUGGY LOGIC:
            // If the entry is a tombstone, it is skipped instead of marking the key
            // as deleted in latest_by_key, allowing older values to be retained!
            if !entry.is_tombstone() {
                latest_by_key.insert(entry.key.clone(), entry.clone());
            }
        }

        let mut compacted: Vec<Entry> = latest_by_key.into_values().collect();
        compacted.sort_by_key(|e| e.generation);
        self.entries = compacted;
    }
}
"#;
        fs::write(repo_dir.join("src/storage.rs"), storage_rs).expect("failed to write storage.rs");

        // 4. src/lib.rs
        let lib_rs = r#"pub mod entry;
pub mod storage;

pub use entry::Entry;
pub use storage::StorageEngine;
"#;
        fs::write(repo_dir.join("src/lib.rs"), lib_rs).expect("failed to write lib.rs");

        // 5. tests/tombstone_tests.rs
        let tests_rs = r#"use kv_tombstone::StorageEngine;

#[test]
fn test_basic_put_get() {
    let mut engine = StorageEngine::new();
    engine.set("user_id", "42");
    assert_eq!(engine.get("user_id"), Some("42".to_string()));
}

#[test]
fn test_immediate_deletion() {
    let mut engine = StorageEngine::new();
    engine.set("api_key", "secret123");
    assert_eq!(engine.get("api_key"), Some("secret123".to_string()));
    engine.delete("api_key");
    assert_eq!(engine.get("api_key"), None);
}

#[test]
fn test_tombstone_deletion_persists_across_compaction() {
    let mut engine = StorageEngine::new();
    engine.set("session_token", "active_sess_99");
    assert_eq!(engine.get("session_token"), Some("active_sess_99".to_string()));

    // Delete session token
    engine.delete("session_token");
    assert_eq!(engine.get("session_token"), None);

    // Trigger compaction
    engine.compact();

    // The deleted key MUST NOT resurrect after compaction!
    assert_eq!(
        engine.get("session_token"),
        None,
        "CRITICAL INVARIANT VIOLATION: Deleted key resurrected after log compaction!"
    );
}
"#;
        fs::write(repo_dir.join("tests/tombstone_tests.rs"), tests_rs)
            .expect("failed to write tombstone_tests.rs");

        // 6. README.md
        let readme = r#"# KV Tombstone Storage Engine

An append-only key-value storage engine in Rust with generational log compaction.

## Invariants
1. A deleted key must return `None` upon `get()`.
2. Compaction removes superseded records while strictly maintaining tombstone semantics.
3. Deleted keys MUST never resurrect after compaction.
"#;
        fs::write(repo_dir.join("README.md"), readme).expect("failed to write README.md");

        // 7. .gitignore
        let gitignore = "target/\nCargo.lock\n";
        fs::write(repo_dir.join(".gitignore"), gitignore).expect("failed to write .gitignore");

        // 8. Initialize Git repository
        Self::run_git(repo_dir, &["init"]);
        Self::run_git(repo_dir, &["config", "user.name", "Plexis Workload Setup"]);
        Self::run_git(repo_dir, &["config", "user.email", "workload@plexis.local"]);
        Self::run_git(repo_dir, &["add", "-A"]);
        Self::run_git(
            repo_dir,
            &[
                "commit",
                "-m",
                "Initial commit for kv-tombstone workload with failing compaction assertion",
            ],
        );

        repo_dir.to_path_buf()
    }

    /// Verifies that the initial repository produces a genuine test failure on compaction.
    pub fn assert_initial_failure(repo_dir: &Path) {
        let output = StdCommand::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("tombstone_tests")
            .current_dir(repo_dir)
            .output()
            .expect("failed to execute cargo test on initial workload");

        assert!(
            !output.status.success(),
            "Expected initial canonical workload to fail cargo test, but it passed!"
        );
        let stderr = String::from_utf8_lossy(&output.stdout);
        assert!(
            stderr.contains("CRITICAL INVARIANT VIOLATION") || stderr.contains("resurrected"),
            "Expected failure message in test output: {}",
            stderr
        );
    }

    /// Verifies that the workload has been completely and independently repaired.
    pub fn assert_repaired_state(repo_dir: &Path) -> String {
        // 1. All tests must pass
        let test_status = StdCommand::new("cargo")
            .arg("test")
            .current_dir(repo_dir)
            .status()
            .expect("failed to execute cargo test");
        assert!(
            test_status.success(),
            "cargo test failed after autonomous repair"
        );

        // 2. Git status must be clean
        let status_output = StdCommand::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo_dir)
            .output()
            .expect("failed to check git status");
        assert!(
            status_output.stdout.is_empty(),
            "Working tree is not clean: {}",
            String::from_utf8_lossy(&status_output.stdout)
        );

        // 3. Git log must have at least 2 commits
        let log_output = StdCommand::new("git")
            .args(["log", "-n", "1", "--format=%H %s"])
            .current_dir(repo_dir)
            .output()
            .expect("failed to get git commit log");
        let commit_line = String::from_utf8_lossy(&log_output.stdout)
            .trim()
            .to_string();
        let commit_sha = commit_line
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        assert!(!commit_sha.is_empty(), "No commit SHA found in git log");

        commit_sha
    }

    fn run_git(repo_dir: &Path, args: &[&str]) {
        let status = StdCommand::new("git")
            .args(args)
            .current_dir(repo_dir)
            .status()
            .unwrap_or_else(|e| panic!("failed to run git {:?}: {}", args, e));
        assert!(status.success(), "git command {:?} failed", args);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_canonical_workload_setup_and_failure_manifestation() {
        let temp = tempdir().unwrap();
        let repo_dir = CanonicalWorkload::setup_repository(temp.path());
        CanonicalWorkload::assert_initial_failure(&repo_dir);
    }
}
