//! Workspace Isolation and Environment Security for Local Agent Host
//!
//! Validates target workspace containment, guards against path traversal,
//! and scrubs secret environment variables before spawning external agent processes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::RuntimeError;

/// Forbidden system roots that external agents may not use as a workspace.
const RESTRICTED_SYSTEM_DIRS: &[&str] = &[
    "/", "/bin", "/sbin", "/etc", "/usr", "/boot", "/dev", "/sys", "/proc", "/root", "/lib",
    "/lib64",
];

/// Validator enforcing workspace boundaries and directory containment.
pub struct WorkspaceValidator;

impl WorkspaceValidator {
    /// Validates that a workspace path exists, is a directory, and is not a restricted system directory.
    pub fn validate_and_canonicalize(path: &Path) -> Result<PathBuf, RuntimeError> {
        if !path.exists() {
            return Err(RuntimeError::InvalidCommand(format!(
                "Workspace path does not exist: {}",
                path.display()
            )));
        }

        if !path.is_dir() {
            return Err(RuntimeError::InvalidCommand(format!(
                "Workspace path is not a directory: {}",
                path.display()
            )));
        }

        // Canonicalize to resolve symlinks and '..' path traversal sequences
        let canonical = path.canonicalize().map_err(|e| {
            RuntimeError::InvalidCommand(format!(
                "Failed to canonicalize workspace path {}: {}",
                path.display(),
                e
            ))
        })?;

        let canonical_str = canonical.to_string_lossy();

        // Guard against selecting restricted system directories
        for restricted in RESTRICTED_SYSTEM_DIRS {
            if canonical_str == *restricted {
                return Err(RuntimeError::Security(format!(
                    "Workspace path '{}' violates boundary: root or system directory forbidden",
                    canonical_str
                )));
            }
        }

        Ok(canonical)
    }
}

/// Environment scrubber removing sensitive host secrets from external child processes.
pub struct EnvironmentScrubber;

impl EnvironmentScrubber {
    /// Keys containing any of these substrings will be stripped from child environment.
    const SENSITIVE_SUBSTRINGS: &[&str] = &[
        "KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "AUTH",
        "PASSWD",
        "CREDENTIAL",
        "PRIVATE",
        "APIKEY",
    ];

    /// Allowed environment variables that may pass through even if they match patterns.
    const ALLOWED_OVERRIDES: &[&str] = &[
        "PLEXIS_AUTH_TOKEN", // Injected specifically if required by authorized tool
    ];

    /// Produces a scrubbed environment map for child process execution.
    pub fn prepare_child_environment(
        custom_vars: &HashMap<String, String>,
        execution_id_str: &str,
        workspace_path: &Path,
    ) -> HashMap<String, String> {
        let mut clean_env = HashMap::new();

        // 1. Inherit safe host variables (PATH, HOME, USER, RUST_LOG, etc.)
        for (key, val) in std::env::vars() {
            let upper = key.to_uppercase();
            let is_sensitive = Self::SENSITIVE_SUBSTRINGS
                .iter()
                .any(|sub| upper.contains(sub));

            if !is_sensitive || Self::ALLOWED_OVERRIDES.contains(&key.as_str()) {
                clean_env.insert(key, val);
            }
        }

        // 2. Overlay explicit custom variables (sanitized)
        for (key, val) in custom_vars {
            clean_env.insert(key.clone(), val.clone());
        }

        // 3. Inject standard telemetry & context variables
        clean_env.insert(
            "PLEXIS_EXECUTION_ID".to_string(),
            execution_id_str.to_string(),
        );
        clean_env.insert(
            "PLEXIS_WORKSPACE".to_string(),
            workspace_path.display().to_string(),
        );
        clean_env.insert("PLEXIS_SUPERVISED".to_string(), "1".to_string());

        clean_env
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_workspace_validator_valid_directory() {
        let dir = tempdir().expect("tempdir");
        let validated = WorkspaceValidator::validate_and_canonicalize(dir.path());
        assert!(validated.is_ok());
    }

    #[test]
    fn test_workspace_validator_nonexistent_directory() {
        let bad_path = PathBuf::from("/nonexistent/directory/path/here/12345");
        let res = WorkspaceValidator::validate_and_canonicalize(&bad_path);
        assert!(res.is_err());
    }

    #[test]
    fn test_workspace_validator_rejects_root() {
        let root = PathBuf::from("/");
        let res = WorkspaceValidator::validate_and_canonicalize(&root);
        assert!(res.is_err());
    }

    #[test]
    fn test_environment_scrubber_removes_secrets() {
        unsafe {
            std::env::set_var("TEST_SECRET_API_KEY", "super_secret_value_123");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "aws_secret_key_456");
            std::env::set_var("REGULAR_VAR", "regular_value");
        }

        let custom = HashMap::new();
        let env = EnvironmentScrubber::prepare_child_environment(
            &custom,
            "exec_123",
            Path::new("/tmp/test"),
        );

        assert!(!env.contains_key("TEST_SECRET_API_KEY"));
        assert!(!env.contains_key("AWS_SECRET_ACCESS_KEY"));
        assert_eq!(
            env.get("REGULAR_VAR").map(|s| s.as_str()),
            Some("regular_value")
        );
        assert_eq!(
            env.get("PLEXIS_EXECUTION_ID").map(|s| s.as_str()),
            Some("exec_123")
        );
        assert_eq!(
            env.get("PLEXIS_WORKSPACE").map(|s| s.as_str()),
            Some("/tmp/test")
        );
    }
}
