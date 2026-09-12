//! Strongly typed identifiers for Plexis entities.
//!
//! All identifiers are 128-bit UUID-backed newtypes with human-readable prefixes
//! (e.g. `task_0194...`). They support serialization, deserialization, display formatting,
//! string parsing, and ordering.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident, $prefix:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a new unique identifier using UUIDv7 (time-ordered).
            #[inline]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Creates an identifier wrapping an existing UUID.
            #[inline]
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Returns the inner UUID.
            #[inline]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }

            /// Returns the type prefix for this identifier.
            #[inline]
            pub const fn prefix() -> &'static str {
                $prefix
            }
        }

        impl Default for $name {
            #[inline]
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0.simple())
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let stripped = if let Some(rest) = s.strip_prefix(concat!($prefix, "_")) {
                    rest
                } else if let Some(rest) = s.strip_prefix(concat!($prefix, "-")) {
                    rest
                } else {
                    s
                };

                let uuid = Uuid::parse_str(stripped).map_err(|e| IdParseError {
                    expected_prefix: $prefix,
                    details: e.to_string(),
                })?;

                Ok(Self(uuid))
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

/// Error returned when parsing an identifier from a string fails.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("failed to parse id for type '{expected_prefix}': {details}")]
pub struct IdParseError {
    pub expected_prefix: &'static str,
    pub details: String,
}

define_id!(TaskId, "task", "Unique identifier for a task.");
define_id!(WorkflowId, "wf", "Unique identifier for a workflow.");
define_id!(AgentId, "agent", "Unique identifier for a logical agent.");
define_id!(
    ExecutionId,
    "exec",
    "Unique identifier for an execution run."
);
define_id!(SessionId, "sess", "Unique identifier for an agent session.");
define_id!(LeaseId, "lease", "Unique identifier for a task lease.");
define_id!(CommandId, "cmd", "Unique identifier for a durable command.");
define_id!(
    ArtifactId,
    "art",
    "Unique identifier for a generated artifact."
);
define_id!(
    VerificationId,
    "verif",
    "Unique identifier for a verification run."
);
define_id!(EventId, "evt", "Unique identifier for an audit event.");
define_id!(
    MessageId,
    "msg",
    "Unique identifier for an agent-to-agent message."
);
define_id!(PlanId, "plan", "Unique identifier for a planning run.");
define_id!(
    ApprovalId,
    "appr",
    "Unique identifier for a human approval gate."
);
define_id!(
    MemoryId,
    "mem",
    "Unique identifier for a persistent memory record."
);
define_id!(
    RecoveryId,
    "rec",
    "Unique identifier for a failure recovery attempt record."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_generation_and_formatting() {
        let task_id = TaskId::new();
        let formatted = task_id.to_string();
        assert!(formatted.starts_with("task_"));

        let parsed: TaskId = formatted.parse().expect("should parse formatted id");
        assert_eq!(task_id, parsed);
    }

    #[test]
    fn test_id_serde_roundtrip() {
        let agent_id = AgentId::new();
        let json = serde_json::to_string(&agent_id).expect("serialize");
        let deserialized: AgentId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(agent_id, deserialized);
    }

    #[test]
    fn test_parse_raw_uuid() {
        let uuid = Uuid::now_v7();
        let raw_str = uuid.to_string();
        let task_id: TaskId = raw_str.parse().expect("should parse raw uuid");
        assert_eq!(task_id.as_uuid(), uuid);
    }
}
