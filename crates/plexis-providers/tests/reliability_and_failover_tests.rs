use std::sync::Arc;
use std::time::Duration;

use plexis_providers::{
    CapabilityRequirement, ChatMessage, CompletionRequest, CompletionResponse, FailoverRouter,
    PrivacyPolicy, ProviderDescriptor, ProviderError, ProviderHealthTracker, RetryPolicy,
    ScriptedProvider,
};

#[tokio::test]
async fn test_provider_failover_on_transient_failure() {
    // 1. Primary provider fails with transient network errors
    let primary_scripted = Arc::new(ScriptedProvider::new("primary_openai"));
    primary_scripted.queue_provider_error(ProviderError::Network(
        "Connection reset by peer".to_string(),
    ));
    primary_scripted.queue_provider_error(ProviderError::Network("Gateway timeout".to_string()));

    // 2. Secondary fallback provider succeeds
    let fallback_response =
        CompletionResponse::text("Successfully completed via secondary provider");
    let secondary_scripted = Arc::new(ScriptedProvider::new("secondary_anthropic"));
    secondary_scripted.queue_response(fallback_response.clone());

    let primary_desc = ProviderDescriptor::new("primary_openai", "gpt-4o", 2, true, false);
    let secondary_desc =
        ProviderDescriptor::new("secondary_anthropic", "claude-3-5-sonnet", 2, true, false);

    let health_tracker = Arc::new(ProviderHealthTracker::new(2, Duration::from_secs(60)));
    // Quick retry policy for fast testing
    let retry_policy = RetryPolicy::new(1, Duration::from_millis(5), Duration::from_millis(20));

    let router = FailoverRouter::new(
        vec![
            (primary_desc, primary_scripted),
            (secondary_desc, secondary_scripted),
        ],
        health_tracker.clone(),
        retry_policy,
    );

    let req = CompletionRequest::new(
        "gpt-4o",
        vec![ChatMessage::user("Solve this autonomous task")],
    );

    let res = router
        .execute_with_failover(
            &req,
            &PrivacyPolicy::Any,
            &CapabilityRequirement::strong_with_tools(),
        )
        .await
        .expect("failover execution");

    assert_eq!(
        res.message.content.as_deref(),
        Some("Successfully completed via secondary provider")
    );

    // Primary provider health should be Degraded or Unavailable after failures
    assert!(!health_tracker.is_available("primary_openai"));
}

#[tokio::test]
async fn test_privacy_policy_enforcement() {
    let cloud_provider = Arc::new(ScriptedProvider::new("openai_cloud"));
    let local_provider = Arc::new(ScriptedProvider::new("ollama_local"));
    local_provider.queue_response(CompletionResponse::text(
        "Running securely in local environment",
    ));

    let cloud_desc = ProviderDescriptor::new("openai_cloud", "gpt-4o", 2, true, false);
    let local_desc = ProviderDescriptor::new("ollama_local", "llama3", 2, true, true);

    let health_tracker = Arc::new(ProviderHealthTracker::default());
    let retry_policy = RetryPolicy::default();

    let router = FailoverRouter::new(
        vec![(cloud_desc, cloud_provider), (local_desc, local_provider)],
        health_tracker,
        retry_policy,
    );

    let req = CompletionRequest::new(
        "llama3",
        vec![ChatMessage::user("Process confidential data")],
    );

    // With LocalOnly privacy policy, cloud_provider must NOT even be selected as a candidate!
    let res = router
        .execute_with_failover(
            &req,
            &PrivacyPolicy::LocalOnly,
            &CapabilityRequirement::fast(),
        )
        .await
        .expect("local execution");

    assert_eq!(
        res.message.content.as_deref(),
        Some("Running securely in local environment")
    );
}

#[tokio::test]
async fn test_capability_downgrade_prevention() {
    // Only a Tier 1 fast model is available
    let weak_provider = Arc::new(ScriptedProvider::new("weak_model"));
    let weak_desc = ProviderDescriptor::new("weak_model", "gpt-3.5-mini", 1, false, false);

    let health_tracker = Arc::new(ProviderHealthTracker::default());
    let retry_policy = RetryPolicy::default();

    let router = FailoverRouter::new(
        vec![(weak_desc, weak_provider)],
        health_tracker,
        retry_policy,
    );

    let req = CompletionRequest::new(
        "complex_model",
        vec![ChatMessage::user("Verify formal invariant proofs")],
    );

    // Require deep reasoning (tier 3) with tool calling
    let strict_req = CapabilityRequirement::deep_reasoning();

    let err = router
        .execute_with_failover(&req, &PrivacyPolicy::Any, &strict_req)
        .await
        .unwrap_err();

    // Must refuse to silently downgrade to Tier 1 model
    assert!(
        matches!(err, ProviderError::Unavailable(_)),
        "Must return ProviderError::Unavailable when no provider meets capability constraints"
    );
}
