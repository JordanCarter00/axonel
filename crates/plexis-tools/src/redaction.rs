use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

static BEARER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9_\-\.]{16,}\b").expect("Valid regex"));

static API_KEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bsk-[A-Za-z0-9_\-]{20,}\b").expect("Valid regex"));

static GOOGLE_KEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bAIza[0-9A-Za-z_\-]{20,50}\b").expect("Valid regex"));

static GH_TOKEN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b").expect("Valid regex"));

static AWS_KEY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bAKIA[0-9A-Z]{16}\b").expect("Valid regex"));

static PRIVATE_KEY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----")
        .expect("Valid regex")
});

/// Utility for redacting sensitive secrets from command output, logs, and json payloads.
pub struct SecretRedactor;

impl SecretRedactor {
    /// Redacts all instances of known secrets from a string.
    /// Secrets shorter than 3 characters are skipped to avoid false positive truncation.
    pub fn redact_text(text: &str, secrets: &[String]) -> String {
        if text.is_empty() || secrets.is_empty() {
            return text.to_string();
        }

        // Sort secrets by descending length so substrings don't preempt longer tokens
        let mut sorted_secrets: Vec<&str> = secrets
            .iter()
            .map(|s| s.as_str())
            .filter(|s| s.len() >= 3)
            .collect();
        sorted_secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));

        let mut redacted = text.to_string();
        for secret in sorted_secrets {
            redacted = redacted.replace(secret, "[REDACTED]");
        }
        redacted
    }

    /// Automatically scans and redacts common credential and token patterns (Bearer, API keys, private keys).
    pub fn redact_patterns(text: &str) -> String {
        if text.is_empty() {
            return text.to_string();
        }

        let step1 = BEARER_REGEX.replace_all(text, "Bearer [REDACTED]");
        let step2 = API_KEY_REGEX.replace_all(&step1, "[REDACTED_API_KEY]");
        let step3 = GOOGLE_KEY_REGEX.replace_all(&step2, "[REDACTED_API_KEY]");
        let step4 = GH_TOKEN_REGEX.replace_all(&step3, "[REDACTED_GH_TOKEN]");
        let step5 = AWS_KEY_REGEX.replace_all(&step4, "[REDACTED_AWS_KEY]");
        let step6 = PRIVATE_KEY_REGEX.replace_all(&step5, "[REDACTED_PRIVATE_KEY]");
        step6.into_owned()
    }

    /// Applies both known explicit secret strings and automatic pattern redaction.
    pub fn redact_all(text: &str, secrets: &[String]) -> String {
        let text_redacted = Self::redact_text(text, secrets);
        Self::redact_patterns(&text_redacted)
    }

    /// Recursively redacts sensitive secrets in-place within a JSON value using explicit list.
    pub fn redact_value(value: &mut Value, secrets: &[String]) {
        if secrets.is_empty() {
            return;
        }

        match value {
            Value::String(s) => {
                *s = Self::redact_text(s, secrets);
            }
            Value::Array(arr) => {
                for item in arr.iter_mut() {
                    Self::redact_value(item, secrets);
                }
            }
            Value::Object(map) => {
                for (_key, val) in map.iter_mut() {
                    Self::redact_value(val, secrets);
                }
            }
            _ => {}
        }
    }

    /// Recursively redacts sensitive secrets and patterns in-place within a JSON value.
    pub fn redact_value_all(value: &mut Value, secrets: &[String]) {
        match value {
            Value::String(s) => {
                *s = Self::redact_all(s, secrets);
            }
            Value::Array(arr) => {
                for item in arr.iter_mut() {
                    Self::redact_value_all(item, secrets);
                }
            }
            Value::Object(map) => {
                for (_key, val) in map.iter_mut() {
                    Self::redact_value_all(val, secrets);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_secret_redaction_text() {
        let secrets = vec![
            "sk-ant-api03-abcdef123456".to_string(),
            "ghp_token987654321".to_string(),
        ];

        let log = "Authenticating with bearer token sk-ant-api03-abcdef123456 to repo with ghp_token987654321!";
        let redacted = SecretRedactor::redact_text(log, &secrets);

        assert_eq!(
            redacted,
            "Authenticating with bearer token [REDACTED] to repo with [REDACTED]!"
        );
        assert!(!redacted.contains("abcdef123456"));
    }

    #[test]
    fn test_secret_redaction_json_recursive() {
        let secrets = vec!["super_secret_db_password".to_string()];

        let mut payload = json!({
            "command": "psql postgres://app:super_secret_db_password@localhost/db",
            "metadata": {
                "env": ["DB_PASS=super_secret_db_password"],
                "count": 42
            }
        });

        SecretRedactor::redact_value(&mut payload, &secrets);

        assert_eq!(
            payload["command"],
            "psql postgres://app:[REDACTED]@localhost/db"
        );
        assert_eq!(payload["metadata"]["env"][0], "DB_PASS=[REDACTED]");
    }

    #[test]
    fn test_pattern_redaction() {
        let text = "Header: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.xyz\nKey: sk-proj-1234567890abcdefghijklmnop\nGitHub: ghp_123456789012345678901234567890\nAWS: AKIAIOSFODNN7EXAMPLE";
        let redacted = SecretRedactor::redact_patterns(text);

        assert!(redacted.contains("Header: Bearer [REDACTED]"));
        assert!(redacted.contains("Key: [REDACTED_API_KEY]"));
        assert!(redacted.contains("GitHub: [REDACTED_GH_TOKEN]"));
        assert!(redacted.contains("AWS: [REDACTED_AWS_KEY]"));
        assert!(!redacted.contains("sk-proj"));
        assert!(!redacted.contains("ghp_"));
        assert!(!redacted.contains("AKIAIOSFODNN7EXAMPLE"));
    }
}
