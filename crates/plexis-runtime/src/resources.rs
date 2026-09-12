use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::RuntimeError;

/// Configuration defining hard resource boundaries below the LLM layer.
#[derive(Debug, Clone)]
pub struct ResourceLimitsConfig {
    pub max_steps_per_task: u32,
    pub max_tool_calls_per_execution: u32,
    pub max_tokens_per_task: u64,
    pub max_duration_per_task: Duration,
    pub max_concurrent_agents: usize,
    pub error_spike_threshold: u32,
}

impl Default for ResourceLimitsConfig {
    fn default() -> Self {
        Self {
            max_steps_per_task: 25,
            max_tool_calls_per_execution: 50,
            max_tokens_per_task: 100_000,
            max_duration_per_task: Duration::from_secs(600), // 10 minutes
            max_concurrent_agents: 10,
            error_spike_threshold: 5,
        }
    }
}

/// Tracks runtime resource consumption for an active task execution.
#[derive(Debug)]
pub struct TaskResourceTracker {
    config: ResourceLimitsConfig,
    start_time: Instant,
    step_count: u32,
    tool_call_count: u32,
    accumulated_tokens: u64,
    consecutive_errors: u32,
}

impl TaskResourceTracker {
    pub fn new(config: ResourceLimitsConfig) -> Self {
        Self {
            config,
            start_time: Instant::now(),
            step_count: 0,
            tool_call_count: 0,
            accumulated_tokens: 0,
            consecutive_errors: 0,
        }
    }

    /// Records an execution step, verifying limits.
    pub fn record_step(&mut self) -> Result<(), RuntimeError> {
        self.step_count += 1;
        self.check_time_limit()?;

        if self.step_count > self.config.max_steps_per_task {
            return Err(RuntimeError::Execution(format!(
                "Resource limit exceeded: Task reached maximum steps ({} > {})",
                self.step_count, self.config.max_steps_per_task
            )));
        }
        Ok(())
    }

    /// Records a tool invocation, verifying limits.
    pub fn record_tool_call(&mut self) -> Result<(), RuntimeError> {
        self.tool_call_count += 1;
        self.check_time_limit()?;

        if self.tool_call_count > self.config.max_tool_calls_per_execution {
            return Err(RuntimeError::Execution(format!(
                "Resource limit exceeded: Execution reached maximum tool calls ({} > {})",
                self.tool_call_count, self.config.max_tool_calls_per_execution
            )));
        }
        Ok(())
    }

    /// Records token consumption, verifying limits.
    pub fn record_tokens(&mut self, tokens: u64) -> Result<(), RuntimeError> {
        self.accumulated_tokens += tokens;

        if self.accumulated_tokens > self.config.max_tokens_per_task {
            return Err(RuntimeError::Execution(format!(
                "Resource limit exceeded: Task accumulated {} tokens (limit: {})",
                self.accumulated_tokens, self.config.max_tokens_per_task
            )));
        }
        Ok(())
    }

    /// Checks if the execution duration has exceeded the maximum limit.
    pub fn check_time_limit(&self) -> Result<(), RuntimeError> {
        let elapsed = self.start_time.elapsed();
        if elapsed > self.config.max_duration_per_task {
            return Err(RuntimeError::Execution(format!(
                "Resource limit exceeded: Wall-clock timeout after {:?} (limit: {:?})",
                elapsed, self.config.max_duration_per_task
            )));
        }
        Ok(())
    }

    /// Records an execution error and checks for circuit breaker trips.
    pub fn record_error(&mut self) -> Result<(), RuntimeError> {
        self.consecutive_errors += 1;
        if self.consecutive_errors >= self.config.error_spike_threshold {
            return Err(RuntimeError::Execution(format!(
                "Circuit breaker tripped: Encountered {} consecutive errors without progress",
                self.consecutive_errors
            )));
        }
        Ok(())
    }

    /// Resets the consecutive error count upon a successful step.
    pub fn record_progress(&mut self) {
        self.consecutive_errors = 0;
    }

    pub fn steps(&self) -> u32 {
        self.step_count
    }

    pub fn tool_calls(&self) -> u32 {
        self.tool_call_count
    }

    pub fn total_tokens(&self) -> u64 {
        self.accumulated_tokens
    }
}

/// Concurrency limiter tracking active agents across workflows.
pub struct ConcurrencyLimiter {
    active_count: AtomicUsize,
    max_concurrent: usize,
}

impl ConcurrencyLimiter {
    pub fn new(max_concurrent: usize) -> Arc<Self> {
        Arc::new(Self {
            active_count: AtomicUsize::new(0),
            max_concurrent,
        })
    }

    /// Attempts to acquire an agent concurrency slot.
    pub fn try_acquire(&self) -> Result<ConcurrencyPermit<'_>, RuntimeError> {
        let current = self.active_count.load(Ordering::SeqCst);
        if current >= self.max_concurrent {
            return Err(RuntimeError::Execution(format!(
                "Workflow concurrency limit reached: {} active agents (max: {})",
                current, self.max_concurrent
            )));
        }

        self.active_count.fetch_add(1, Ordering::SeqCst);
        Ok(ConcurrencyPermit {
            active_count: &self.active_count,
        })
    }

    pub fn active_agents(&self) -> usize {
        self.active_count.load(Ordering::SeqCst)
    }
}

/// RAII guard decrementing active agent count on drop.
pub struct ConcurrencyPermit<'a> {
    active_count: &'a AtomicUsize,
}

impl<'a> Drop for ConcurrencyPermit<'a> {
    fn drop(&mut self) {
        self.active_count.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_tracker_step_and_tool_limits() {
        let config = ResourceLimitsConfig {
            max_steps_per_task: 3,
            max_tool_calls_per_execution: 2,
            ..Default::default()
        };

        let mut tracker = TaskResourceTracker::new(config);

        assert!(tracker.record_step().is_ok());
        assert!(tracker.record_step().is_ok());
        assert!(tracker.record_step().is_ok());
        assert!(tracker.record_step().is_err()); // Exceeded 3 steps

        assert!(tracker.record_tool_call().is_ok());
        assert!(tracker.record_tool_call().is_ok());
        assert!(tracker.record_tool_call().is_err()); // Exceeded 2 tool calls
    }

    #[test]
    fn test_concurrency_limiter() {
        let limiter = ConcurrencyLimiter::new(2);

        let permit1 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.active_agents(), 1);

        let permit2 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.active_agents(), 2);

        // Third should fail
        assert!(limiter.try_acquire().is_err());

        // Drop permit1
        drop(permit1);
        assert_eq!(limiter.active_agents(), 1);

        // Now can acquire again
        let _permit3 = limiter.try_acquire().unwrap();
        assert_eq!(limiter.active_agents(), 2);

        drop(permit2);
    }
}
