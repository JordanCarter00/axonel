use std::time::Duration;
use tracing::warn;

use crate::error::ProviderError;

/// Exponential backoff and retry policy for provider interactions.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryPolicy {
    pub fn new(max_retries: u32, initial_backoff: Duration, max_backoff: Duration) -> Self {
        Self {
            max_retries,
            initial_backoff,
            max_backoff,
            backoff_multiplier: 2.0,
        }
    }

    /// Computes the duration to wait before retry `attempt` (0-indexed).
    /// Returns `None` if the error is non-transient or the maximum retry limit has been exceeded.
    pub fn compute_delay(&self, attempt: u32, error: &ProviderError) -> Option<Duration> {
        if attempt >= self.max_retries {
            return None;
        }

        if !error.is_transient() {
            warn!(
                attempt,
                error = %error,
                "Error is non-transient; aborting retries"
            );
            return None;
        }

        // Respect explicit Retry-After headers when signaled
        if let Some(explicit) = error.retry_after() {
            let delay = explicit.max(self.initial_backoff).min(self.max_backoff);
            return Some(delay);
        }

        let factor = self.backoff_multiplier.powi(attempt as i32);
        let base_millis = (self.initial_backoff.as_millis() as f64) * factor;
        // Jitter: add up to 10% deterministic variation based on attempt to avoid synchronized thundering herds
        let jitter_millis = base_millis * 0.1 * ((attempt % 3) as f64);
        let total_millis = (base_millis + jitter_millis) as u64;

        let delay = Duration::from_millis(total_millis).min(self.max_backoff);
        Some(delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_policy_transient_vs_fatal() {
        let policy = RetryPolicy::default();

        let transient = ProviderError::Network("connection reset".to_string());
        let fatal = ProviderError::Authentication("invalid api key".to_string());
        let rate_limited = ProviderError::RateLimited {
            retry_after_secs: Some(5),
        };

        // Fatal errors must not retry
        assert!(policy.compute_delay(0, &fatal).is_none());

        // Transient errors should retry up to max_retries
        assert!(policy.compute_delay(0, &transient).is_some());
        assert!(policy.compute_delay(1, &transient).is_some());
        assert!(policy.compute_delay(2, &transient).is_some());
        assert!(policy.compute_delay(3, &transient).is_none());

        // Rate limited respects Retry-After
        let delay = policy.compute_delay(0, &rate_limited).unwrap();
        assert_eq!(delay, Duration::from_secs(5));
    }
}
