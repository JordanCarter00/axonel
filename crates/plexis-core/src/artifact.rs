//! Durable Artifact domain model for Plexis.
//!
//! Artifacts are immutable outputs produced by agents during task execution,
//! such as code patches, generated files, test logs, or reports.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AgentId, ArtifactId, TaskId};

/// Durable record of an output artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Unique artifact identifier.
    pub id: ArtifactId,
    /// Task that produced this artifact.
    pub task_id: TaskId,
    /// Agent that produced this artifact.
    pub created_by: AgentId,
    /// Logical name or label.
    pub name: String,
    /// Relative or absolute path / URI where the artifact content resides.
    pub path_or_uri: String,
    /// Content hash (e.g. SHA-256) for integrity checking.
    pub checksum: Option<String>,
    /// Structured metadata (size, mime type, etc.).
    pub metadata: serde_json::Value,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl Artifact {
    pub fn new(
        task_id: TaskId,
        created_by: AgentId,
        name: impl Into<String>,
        path_or_uri: impl Into<String>,
    ) -> Self {
        Self {
            id: ArtifactId::new(),
            task_id,
            created_by,
            name: name.into(),
            path_or_uri: path_or_uri.into(),
            checksum: None,
            metadata: serde_json::Value::Object(Default::default()),
            created_at: Utc::now(),
        }
    }

    pub fn with_checksum(mut self, checksum: impl Into<String>) -> Self {
        self.checksum = Some(checksum.into());
        self
    }
}
