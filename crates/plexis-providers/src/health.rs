use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::error::ProviderError;

/// Health status of a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderHealthStatus {
    Healthy,
    Degraded { consecutive_failures: u32 },
    Unavailable { until: Instant, reason: String },
}

#[derive(Debug, Clone)]
struct HealthEntry {
    status: ProviderHealthStatus,
    consecutive_failures: u32,
    total_successes: u64,
    total_failures: u64,
    last_error: Option<String>,
}

impl Default for HealthEntry {
    fn default() -> Self {
        Self {
            status: ProviderHealthStatus::Healthy,
            consecutive_failures: 0,
            total_successes: 0,
            total_failures: 0,
            last_error: None,
        }
    }
}

/// Tracks provider availability, failure rates, and circuit-breaker states.
pub struct ProviderHealthTracker {
    entries: RwLock<HashMap<String, HealthEntry>>,
    failure_threshold: u32,
    cooldown_duration: Duration,
}

impl Default for ProviderHealthTracker {
    fn default() -> Self {
        Self::new(3, Duration::from_secs(60))
    }
}

impl ProviderHealthTracker {
    pub fn new(failure_threshold: u32, cooldown_duration: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            failure_threshold,
            cooldown_duration,
        }
    }

    /// Records a successful interaction with a provider.
    pub fn record_success(&self, provider_id: &str) {
        let mut map = self.entries.write().unwrap();
        let entry = map.entry(provider_id.to_string()).or_default();
        entry.consecutive_failures = 0;
        entry.total_successes += 1;
        entry.status = ProviderHealthStatus::Healthy;
    }

    /// Records a failure when interacting with a provider.
    pub fn record_failure(&self, provider_id: &str, error: &ProviderError) {
        let mut map = self.entries.write().unwrap();
        let entry = map.entry(provider_id.to_string()).or_default();
        entry.consecutive_failures += 1;
        entry.total_failures += 1;
        entry.last_error = Some(error.to_string());

        let now = Instant::now();

        // If rate limited with Retry-After, enter Unavailable state for that duration
        if let Some(retry_after) = error.retry_after() {
            let until = now + retry_after;
            warn!(
                provider = %provider_id,
                consecutive = entry.consecutive_failures,
                "Provider rate-limited; marking unavailable until retry-after expiry"
            );
            entry.status = ProviderHealthStatus::Unavailable {
                until,
                reason: "Rate limited by provider".to_string(),
            };
            return;
        }

        // If failures exceed threshold, trip circuit breaker
        if entry.consecutive_failures >= self.failure_threshold {
            let until = now + self.cooldown_duration;
            warn!(
                provider = %provider_id,
                consecutive = entry.consecutive_failures,
                "Provider exceeded consecutive failure threshold; tripping circuit breaker"
            );
            entry.status = ProviderHealthStatus::Unavailable {
                until,
                reason: format!(
                    "Tripped circuit breaker after {} failures",
                    entry.consecutive_failures
                ),
            };
        } else {
            entry.status = ProviderHealthStatus::Degraded {
                consecutive_failures: entry.consecutive_failures,
            };
        }
    }

    /// Checks if a provider is currently available to accept requests.
    pub fn is_available(&self, provider_id: &str) -> bool {
        let mut map = self.entries.write().unwrap();
        let entry = map.entry(provider_id.to_string()).or_default();

        match &entry.status {
            ProviderHealthStatus::Healthy => true,
            ProviderHealthStatus::Degraded { .. } => true,
            ProviderHealthStatus::Unavailable { until, .. } => {
                if Instant::now() >= *until {
                    info!(provider = %provider_id, "Provider cooldown expired; permitting half-open probe");
                    entry.status = ProviderHealthStatus::Degraded {
                        consecutive_failures: entry.consecutive_failures,
                    };
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Gets the current health status of a provider.
    pub fn get_status(&self, provider_id: &str) -> ProviderHealthStatus {
        let map = self.entries.read().unwrap();
        map.get(provider_id)
            .map(|e| e.status.clone())
            .unwrap_or(ProviderHealthStatus::Healthy)
    }
}
