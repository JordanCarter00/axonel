//! Gemini CLI Stream Translation Protocol
//!
//! Parses and translates structured line-delimited events from Gemini CLI
//! into Plexis `ExecutionEvent` structures.

use plexis_core::ids::ExecutionId;
use plexis_core::protocol::ExecutionEvent;
use serde_json::Value;

use crate::agent_host::OutputParser;

/// Translates Gemini CLI streaming outputs into Plexis protocol events.
#[derive(Debug, Default, Clone)]
pub struct GeminiStreamParser;

impl GeminiStreamParser {
    pub fn new() -> Self {
        Self
    }
}

impl OutputParser for GeminiStreamParser {
    fn parse_line(&self, execution_id: ExecutionId, line: &str) -> Option<ExecutionEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // 1. Attempt to parse line as a JSON object (from --output-format stream-json)
        if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
            if let Some(obj) = val.as_object() {
                // Event type: tool_call / tool_use / tool
                if let Some(tool_name) = obj
                    .get("tool")
                    .or_else(|| obj.get("tool_name"))
                    .or_else(|| obj.get("name"))
                    .and_then(|v| v.as_str())
                {
                    let action = obj
                        .get("action")
                        .or_else(|| obj.get("command"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("execute");
                    let details = obj
                        .get("parameters")
                        .or_else(|| obj.get("args"))
                        .or_else(|| obj.get("details"))
                        .cloned()
                        .unwrap_or_else(|| val.clone());

                    return Some(ExecutionEvent::tool_action(
                        execution_id,
                        tool_name,
                        action,
                        details,
                    ));
                }

                // Event type: error
                if let Some(err) = obj.get("error") {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .or_else(|| err.as_str())
                        .unwrap_or("Unknown Gemini error");
                    return Some(ExecutionEvent::warning(
                        execution_id,
                        format!("Gemini diagnostic: {}", msg),
                    ));
                }

                // Event type: status / progress
                if let Some(status) = obj.get("status").and_then(|v| v.as_str()) {
                    let pct = obj
                        .get("percentage")
                        .and_then(|p| p.as_f64())
                        .unwrap_or(0.5) as f32;
                    return Some(ExecutionEvent::progress(execution_id, pct, status));
                }

                // Text / content chunk
                if let Some(content) = obj
                    .get("text")
                    .or_else(|| obj.get("content"))
                    .or_else(|| obj.get("delta"))
                    .or_else(|| obj.get("response"))
                    .and_then(|v| v.as_str())
                {
                    return Some(ExecutionEvent::stdout(execution_id, content));
                }
            }
        }

        // 2. Fallback heuristics for non-JSON lines (e.g. CLI notices)
        if trimmed.starts_with("Warning:") {
            return Some(ExecutionEvent::warning(execution_id, trimmed));
        }

        // Default to standard stdout event
        Some(ExecutionEvent::stdout(execution_id, trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plexis_core::protocol::ExecutionEventType;

    #[test]
    fn test_parse_tool_action() {
        let parser = GeminiStreamParser::new();
        let exec_id = ExecutionId::new();
        let json =
            r#"{"tool": "file_edit", "action": "replace", "parameters": {"path": "src/lib.rs"}}"#;

        let ev = parser.parse_line(exec_id, json).expect("parsed event");
        assert_eq!(ev.execution_id, exec_id);
        match ev.event {
            ExecutionEventType::ToolAction { tool, action, .. } => {
                assert_eq!(tool, "file_edit");
                assert_eq!(action, "replace");
            }
            _ => panic!("Expected ToolAction, got {:?}", ev.event),
        }
    }

    #[test]
    fn test_parse_content_delta() {
        let parser = GeminiStreamParser::new();
        let exec_id = ExecutionId::new();
        let json = r#"{"text": "Refactoring multiply function to return a * b"}"#;

        let ev = parser.parse_line(exec_id, json).expect("parsed event");
        match ev.event {
            ExecutionEventType::Stdout { text } => {
                assert!(text.contains("Refactoring multiply function"));
            }
            _ => panic!("Expected Stdout, got {:?}", ev.event),
        }
    }

    #[test]
    fn test_parse_plain_text_fallback() {
        let parser = GeminiStreamParser::new();
        let exec_id = ExecutionId::new();
        let text = "Running cargo test...";

        let ev = parser.parse_line(exec_id, text).expect("parsed event");
        match ev.event {
            ExecutionEventType::Stdout { text: t } => {
                assert_eq!(t, "Running cargo test...");
            }
            _ => panic!("Expected Stdout, got {:?}", ev.event),
        }
    }
}
