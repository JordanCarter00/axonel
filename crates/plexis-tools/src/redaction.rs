use serde_json::Value;

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

    /// Recursively redacts sensitive secrets in-place within a JSON value.
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
}
