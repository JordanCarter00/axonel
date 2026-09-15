//! Provider execution telemetry recording and live provider probe harness.
//!
//! Tracks exact request metrics, tool-call counts, token accounting, latencies,
//! and retries, while clearly differentiating between real provider execution,
//! deterministic simulation, and skipped credentials.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use crate::types::CompletionResponse;

/// Execution mode for a provider request or workflow attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Genuinely executed against a live remote or local AI provider endpoint.
    RealProvider,
    /// Executed using deterministic, repeatable local simulation or scripted provider.
    #[default]
    DeterministicSimulation,
    /// Skipped because external credentials/service were unavailable in environment.
    SkippedUnavailable,
}

impl std::fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RealProvider => write!(f, "RealProvider"),
            Self::DeterministicSimulation => write!(f, "DeterministicSimulation"),
            Self::SkippedUnavailable => write!(f, "SkippedUnavailable"),
        }
    }
}

/// Recorded telemetry for a provider execution run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderExecutionTelemetry {
    pub provider: String,
    pub model: String,
    pub mode: ExecutionMode,
    pub request_count: u32,
    pub tool_call_count: u32,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub estimated_cost_usd: f64,
    pub total_latency_ms: u64,
    pub average_latency_ms: f64,
    pub retry_count: u32,
    pub failures: Vec<String>,
    pub final_status: String,
}

impl ProviderExecutionTelemetry {
    pub fn new(provider: impl Into<String>, model: impl Into<String>, mode: ExecutionMode) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            mode,
            request_count: 0,
            tool_call_count: 0,
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            estimated_cost_usd: 0.0,
            total_latency_ms: 0,
            average_latency_ms: 0.0,
            retry_count: 0,
            failures: Vec::new(),
            final_status: "Initialized".to_string(),
        }
    }

    /// Renders a markdown summary row for run observability tables.
    pub fn to_markdown_summary(&self) -> String {
        format!(
            "| `{}` | `{}` | `{}` | {} | {} | {} (in: {}, out: {}) | ~${:.4} | {} ms (avg: {:.1} ms) | {} | {} | **{}** |",
            self.provider,
            self.model,
            self.mode,
            self.request_count,
            self.tool_call_count,
            self.total_tokens,
            self.prompt_tokens,
            self.completion_tokens,
            self.estimated_cost_usd,
            self.total_latency_ms,
            self.average_latency_ms,
            self.retry_count,
            self.failures.len(),
            self.final_status
        )
    }
}

/// Thread-safe telemetry recorder that tracks provider activity.
#[derive(Debug, Default, Clone)]
pub struct ProviderTelemetryRecorder {
    inner: Arc<RwLock<ProviderExecutionTelemetry>>,
}

impl ProviderTelemetryRecorder {
    pub fn new(provider: impl Into<String>, model: impl Into<String>, mode: ExecutionMode) -> Self {
        Self {
            inner: Arc::new(RwLock::new(ProviderExecutionTelemetry::new(
                provider, model, mode,
            ))),
        }
    }

    /// Records the start of a request timer.
    pub fn start_request(&self) -> Instant {
        let mut guard = self.inner.write().unwrap();
        guard.request_count += 1;
        Instant::now()
    }

    /// Records a successful completion response with latency and tokens.
    pub fn record_success(&self, start_time: Instant, response: &CompletionResponse) {
        let elapsed = start_time.elapsed().as_millis() as u64;
        let mut guard = self.inner.write().unwrap();
        guard.total_latency_ms += elapsed;
        if guard.request_count > 0 {
            guard.average_latency_ms = guard.total_latency_ms as f64 / guard.request_count as f64;
        }

        let prompt_tokens = response.usage.prompt_tokens;
        let completion_tokens = response.usage.completion_tokens;
        let total_tokens = response.usage.total_tokens;

        guard.prompt_tokens += prompt_tokens;
        guard.completion_tokens += completion_tokens;
        guard.total_tokens += total_tokens;

        // Approximate token cost estimation
        let prompt_rate = 0.0000015; // $1.50 per 1M tokens
        let completion_rate = 0.0000060; // $6.00 per 1M tokens
        guard.estimated_cost_usd +=
            (prompt_tokens as f64 * prompt_rate) + (completion_tokens as f64 * completion_rate);

        if let Some(ref tool_calls) = response.message.tool_calls {
            guard.tool_call_count += tool_calls.len() as u32;
        }

        guard.final_status = "Success".to_string();
    }

    /// Records a failure or retry attempt.
    pub fn record_failure(&self, start_time: Instant, error_msg: &str, is_retry: bool) {
        let elapsed = start_time.elapsed().as_millis() as u64;
        let mut guard = self.inner.write().unwrap();
        guard.total_latency_ms += elapsed;
        if is_retry {
            guard.retry_count += 1;
        }
        guard.failures.push(error_msg.to_string());
        guard.final_status = format!("Failed: {}", error_msg);
    }

    /// Explicitly sets the final status.
    pub fn set_final_status(&self, status: impl Into<String>) {
        let mut guard = self.inner.write().unwrap();
        guard.final_status = status.into();
    }

    /// Takes a snapshot of the current telemetry.
    pub fn snapshot(&self) -> ProviderExecutionTelemetry {
        self.inner.read().unwrap().clone()
    }
}

/// Live provider discovery and verification probe.
pub struct LiveProviderProbe;

impl LiveProviderProbe {
    /// Probes environment variables to detect available live provider credentials.
    pub fn probe_environment() -> Vec<(&'static str, bool)> {
        vec![
            ("openai", std::env::var("OPENAI_API_KEY").is_ok()),
            ("gemini", std::env::var("GEMINI_API_KEY").is_ok()),
            ("anthropic", std::env::var("ANTHROPIC_API_KEY").is_ok()),
            ("ollama", std::env::var("OLLAMA_HOST").is_ok()),
        ]
    }

    /// Returns the primary active provider name and whether it is a live provider.
    pub fn resolve_active_provider() -> (String, String, ExecutionMode) {
        if std::env::var("GEMINI_API_KEY").is_ok() {
            (
                "gemini".into(),
                "gemini-1.5-flash".into(),
                ExecutionMode::RealProvider,
            )
        } else if std::env::var("OPENAI_API_KEY").is_ok() {
            (
                "openai".into(),
                "gpt-4o-mini".into(),
                ExecutionMode::RealProvider,
            )
        } else if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            (
                "anthropic".into(),
                "claude-3-5-sonnet-20241022".into(),
                ExecutionMode::RealProvider,
            )
        } else if std::env::var("OLLAMA_HOST").is_ok() {
            (
                "ollama".into(),
                "llama3:latest".into(),
                ExecutionMode::RealProvider,
            )
        } else {
            (
                "deterministic_simulation".into(),
                "hermetic_simulation_v1".into(),
                ExecutionMode::DeterministicSimulation,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FinishReason, TokenUsage};

    #[test]
    fn test_telemetry_recording() {
        let recorder = ProviderTelemetryRecorder::new(
            "test_provider",
            "test_model",
            ExecutionMode::RealProvider,
        );

        let t0 = recorder.start_request();
        let resp = CompletionResponse {
            message: crate::types::ChatMessage::assistant_with_tools(vec![
                crate::types::ToolCall {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    arguments: "{}".into(),
                },
            ]),
            finish_reason: FinishReason::ToolCalls,
            usage: TokenUsage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
            },
        };

        recorder.record_success(t0, &resp);

        let snap = recorder.snapshot();
        assert_eq!(snap.request_count, 1);
        assert_eq!(snap.tool_call_count, 1);
        assert_eq!(snap.prompt_tokens, 100);
        assert_eq!(snap.completion_tokens, 50);
        assert_eq!(snap.total_tokens, 150);
        assert_eq!(snap.final_status, "Success");
        assert!(snap.estimated_cost_usd > 0.0);
    }
}
