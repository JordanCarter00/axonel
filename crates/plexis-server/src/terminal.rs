//! In-memory streaming terminal output buffer for task execution.
//!
//! Captures live command outputs, logs, stdout/stderr, and exit codes for
//! task execution observability, automatically applying secret redaction.

use chrono::{DateTime, Utc};
use plexis_core::ids::TaskId;
use plexis_tools::redaction::SecretRedactor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// Single line or block of terminal output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalLine {
    pub timestamp: DateTime<Utc>,
    pub stream: String, // "stdout", "stderr", "system"
    pub line: String,
}

/// Aggregated terminal output for a task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskTerminal {
    pub task_id: String,
    pub lines: Vec<TerminalLine>,
    pub exit_code: Option<i32>,
    pub is_completed: bool,
}

/// In-memory ring/buffer storing recent execution terminals.
pub struct TerminalBuffer {
    buffers: RwLock<HashMap<TaskId, TaskTerminal>>,
}

impl Default for TerminalBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalBuffer {
    pub fn new() -> Self {
        Self {
            buffers: RwLock::new(HashMap::new()),
        }
    }

    /// Appends output to the task's terminal buffer with automatic secret pattern redaction.
    pub fn append(&self, task_id: &TaskId, stream: &str, raw_text: &str) {
        let clean_text = SecretRedactor::redact_patterns(raw_text);
        let now = Utc::now();

        let mut lock = self.buffers.write().unwrap();
        let term = lock.entry(*task_id).or_insert_with(|| TaskTerminal {
            task_id: task_id.to_string(),
            lines: Vec::new(),
            exit_code: None,
            is_completed: false,
        });

        for line in clean_text.lines() {
            term.lines.push(TerminalLine {
                timestamp: now,
                stream: stream.to_string(),
                line: line.to_string(),
            });
        }
    }

    /// Records completion and exit code.
    pub fn complete(&self, task_id: &TaskId, exit_code: i32) {
        let mut lock = self.buffers.write().unwrap();
        let term = lock.entry(*task_id).or_insert_with(|| TaskTerminal {
            task_id: task_id.to_string(),
            lines: Vec::new(),
            exit_code: None,
            is_completed: false,
        });
        term.exit_code = Some(exit_code);
        term.is_completed = true;
    }

    /// Retrieves the terminal snapshot for a task.
    pub fn get(&self, task_id: &TaskId) -> Option<TaskTerminal> {
        let lock = self.buffers.read().unwrap();
        lock.get(task_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_buffer_append_and_redaction() {
        let buffer = TerminalBuffer::new();
        let task_id = TaskId::new();

        buffer.append(&task_id, "stdout", "Compiling package v0.1.0");
        buffer.append(
            &task_id,
            "stdout",
            "Authorization: Bearer sk-ant-api03-abcdef123456",
        );
        buffer.complete(&task_id, 0);

        let term = buffer.get(&task_id).expect("found terminal");
        assert_eq!(term.lines.len(), 2);
        assert_eq!(term.exit_code, Some(0));
        assert!(term.is_completed);
        assert!(term.lines[1].line.contains("[REDACTED]"));
        assert!(!term.lines[1].line.contains("sk-ant-api03"));
    }
}
