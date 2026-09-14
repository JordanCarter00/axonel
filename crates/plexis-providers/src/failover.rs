use async_trait::async_trait;
use std::sync::Arc;
use tracing::{info, warn};

use crate::error::ProviderError;
use crate::health::ProviderHealthTracker;
use crate::reliability::RetryPolicy;
use crate::traits::Provider;
use crate::types::{CompletionRequest, CompletionResponse};

/// Privacy constraint governing where prompts and agent data may be transmitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyPolicy {
    /// Strictly forbid cloud transmission; prompts must execute exclusively on local backends (e.g. Ollama).
    LocalOnly,
    /// Only explicitly whitelisted provider IDs are permitted.
    AllowedProviders(Vec<String>),
    /// Prompts may be routed to any configured provider.
    Any,
}

impl PrivacyPolicy {
    pub fn allows(&self, provider_id: &str, is_local: bool) -> bool {
        match self {
            Self::LocalOnly => is_local,
            Self::AllowedProviders(allowed) => allowed.iter().any(|p| p == provider_id),
            Self::Any => true,
        }
    }
}

/// Hard capability requirements for a task that must NOT be silently downgraded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRequirement {
    pub min_reasoning_tier: u8,
    pub requires_tools: bool,
}

impl Default for CapabilityRequirement {
    fn default() -> Self {
        Self {
            min_reasoning_tier: 1,
            requires_tools: false,
        }
    }
}

impl CapabilityRequirement {
    pub fn new(min_tier: u8, requires_tools: bool) -> Self {
        Self {
            min_reasoning_tier: min_tier,
            requires_tools,
        }
    }

    pub fn fast() -> Self {
        Self::new(1, false)
    }

    pub fn strong_with_tools() -> Self {
        Self::new(2, true)
    }

    pub fn deep_reasoning() -> Self {
        Self::new(3, true)
    }
}

/// Metadata descriptor declaring a provider's capabilities, reasoning power, and privacy domain.
#[derive(Debug, Clone)]
pub struct ProviderDescriptor {
    pub provider_id: String,
    pub model_name: String,
    pub reasoning_tier: u8,
    pub supports_tools: bool,
    pub is_local: bool,
}

impl ProviderDescriptor {
    pub fn new(
        provider_id: impl Into<String>,
        model_name: impl Into<String>,
        reasoning_tier: u8,
        supports_tools: bool,
        is_local: bool,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_name: model_name.into(),
            reasoning_tier,
            supports_tools,
            is_local,
        }
    }

    /// Validates if this provider satisfies the required capability and privacy boundaries.
    pub fn satisfies(&self, privacy: &PrivacyPolicy, req: &CapabilityRequirement) -> bool {
        if !privacy.allows(&self.provider_id, self.is_local) {
            return false;
        }

        if self.reasoning_tier < req.min_reasoning_tier {
            return false;
        }

        if req.requires_tools && !self.supports_tools {
            return false;
        }

        true
    }
}

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Record of a failover decision transition from one provider to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverDecision {
    pub from_provider_id: String,
    pub to_provider_id: String,
    pub failure_reason: String,
    pub timestamp: DateTime<Utc>,
}

/// Multi-provider failover router combining capability enforcement, privacy boundary validation,
/// health tracking, and exponential retry.
pub struct FailoverRouter {
    providers: Vec<(ProviderDescriptor, Arc<dyn Provider>)>,
    health_tracker: Arc<ProviderHealthTracker>,
    retry_policy: RetryPolicy,
    decisions: Arc<Mutex<Vec<FailoverDecision>>>,
}

impl FailoverRouter {
    pub fn new(
        providers: Vec<(ProviderDescriptor, Arc<dyn Provider>)>,
        health_tracker: Arc<ProviderHealthTracker>,
        retry_policy: RetryPolicy,
    ) -> Self {
        Self {
            providers,
            health_tracker,
            retry_policy,
            decisions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn get_failover_decisions(&self) -> Vec<FailoverDecision> {
        self.decisions.lock().await.clone()
    }

    /// Selects an ordered list of candidate providers satisfying privacy, capability, and health status.
    pub fn select_candidates(
        &self,
        privacy: &PrivacyPolicy,
        req: &CapabilityRequirement,
    ) -> Vec<(ProviderDescriptor, Arc<dyn Provider>)> {
        self.providers
            .iter()
            .filter(|(desc, _)| desc.satisfies(privacy, req))
            .filter(|(desc, _)| self.health_tracker.is_available(&desc.provider_id))
            .cloned()
            .collect()
    }

    /// Dispatches a completion request through eligible providers in priority order with retries.
    pub async fn execute_with_failover(
        &self,
        request: &CompletionRequest,
        privacy: &PrivacyPolicy,
        capability: &CapabilityRequirement,
    ) -> Result<CompletionResponse, ProviderError> {
        let candidates = self.select_candidates(privacy, capability);

        if candidates.is_empty() {
            return Err(ProviderError::Unavailable(
                "No eligible provider available satisfying capability, privacy, and health requirements".to_string(),
            ));
        }

        let mut last_error = None;
        let mut previous_failure: Option<(String, String)> = None;

        for (desc, provider) in candidates {
            if let Some((from_id, reason)) = previous_failure.take() {
                info!(
                    from_provider = %from_id,
                    to_provider = %desc.provider_id,
                    reason = %reason,
                    "Recording failover decision transition"
                );
                let decision = FailoverDecision {
                    from_provider_id: from_id,
                    to_provider_id: desc.provider_id.clone(),
                    failure_reason: reason,
                    timestamp: Utc::now(),
                };
                self.decisions.lock().await.push(decision);
            }

            info!(
                provider = %desc.provider_id,
                model = %desc.model_name,
                tier = desc.reasoning_tier,
                "Attempting provider dispatch"
            );

            // Attempt execution with retries
            let mut attempt = 0;
            loop {
                match provider.complete(request).await {
                    Ok(response) => {
                        self.health_tracker.record_success(&desc.provider_id);
                        return Ok(response);
                    }
                    Err(err) => {
                        warn!(
                            provider = %desc.provider_id,
                            attempt,
                            error = %err,
                            "Provider request failed"
                        );
                        self.health_tracker.record_failure(&desc.provider_id, &err);

                        if let Some(delay) = self.retry_policy.compute_delay(attempt, &err) {
                            tokio::time::sleep(delay).await;
                            attempt += 1;
                        } else {
                            // Max retries reached or non-transient: failover to next candidate
                            previous_failure = Some((desc.provider_id.clone(), err.to_string()));
                            last_error = Some(err);
                            break;
                        }
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ProviderError::Unavailable("All candidate providers failed".to_string())
        }))
    }
}

#[async_trait]
impl Provider for FailoverRouter {
    fn id(&self) -> &str {
        "failover"
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        self.execute_with_failover(
            request,
            &PrivacyPolicy::Any,
            &CapabilityRequirement::default(),
        )
        .await
    }
}
