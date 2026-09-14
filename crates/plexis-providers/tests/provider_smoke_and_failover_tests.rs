use std::sync::Arc;
use std::time::Duration;

use plexis_providers::adapters::ScriptedProvider;
use plexis_providers::error::ProviderError;
use plexis_providers::failover::{
    CapabilityRequirement, FailoverRouter, PrivacyPolicy, ProviderDescriptor,
};
use plexis_providers::health::ProviderHealthTracker;
use plexis_providers::reliability::RetryPolicy;
use plexis_providers::types::{ChatMessage, CompletionRequest, CompletionResponse};

#[tokio::test]
async fn test_failover_decision_recorded_with_transition_details() {
    let health_tracker = Arc::new(ProviderHealthTracker::new(2, Duration::from_secs(60)));
    let retry_policy = RetryPolicy::new(0, Duration::from_millis(1), Duration::from_millis(1));

    // Provider 1: Fails immediately with simulated 503
    let p1 = Arc::new(ScriptedProvider::new("p1-mock"));
    p1.queue_provider_error(ProviderError::Unavailable(
        "503 Service Unavailable".to_string(),
    ));
    let desc1 = ProviderDescriptor::new("p1-mock", "model-1", 1, false, false);

    // Provider 2: Succeeds
    let p2 = Arc::new(ScriptedProvider::new("p2-mock"));
    p2.queue_response(CompletionResponse::text("Recovered via failover"));
    let desc2 = ProviderDescriptor::new("p2-mock", "model-2", 1, false, false);

    let router = FailoverRouter::new(vec![(desc1, p1), (desc2, p2)], health_tracker, retry_policy);

    let req = CompletionRequest::new("model-1", vec![ChatMessage::user("Hello")]);
    let res = router
        .execute_with_failover(&req, &PrivacyPolicy::Any, &CapabilityRequirement::fast())
        .await
        .unwrap();

    assert_eq!(
        res.message.content.as_deref(),
        Some("Recovered via failover")
    );

    // Verify structured failover decision event was recorded
    let decisions = router.get_failover_decisions().await;
    assert_eq!(
        decisions.len(),
        1,
        "Exactly one failover transition must be recorded"
    );

    let decision = &decisions[0];
    assert_eq!(decision.from_provider_id, "p1-mock");
    assert_eq!(decision.to_provider_id, "p2-mock");
    assert!(
        decision.failure_reason.contains("503 Service Unavailable"),
        "Failure reason must include provider error: {}",
        decision.failure_reason
    );
}

#[tokio::test]
async fn test_exhausted_candidates_error_surfaced() {
    let health_tracker = Arc::new(ProviderHealthTracker::new(2, Duration::from_secs(60)));
    let retry_policy = RetryPolicy::new(0, Duration::from_millis(1), Duration::from_millis(1));

    let p1 = Arc::new(ScriptedProvider::new("p1-mock"));
    p1.queue_provider_error(ProviderError::ExecutionError(
        "500 Internal Error".to_string(),
    ));
    let desc1 = ProviderDescriptor::new("p1-mock", "model-1", 1, false, false);

    let router = FailoverRouter::new(vec![(desc1, p1)], health_tracker, retry_policy);

    let req = CompletionRequest::new("model-1", vec![ChatMessage::user("Test")]);
    let res = router
        .execute_with_failover(&req, &PrivacyPolicy::Any, &CapabilityRequirement::fast())
        .await;

    assert!(res.is_err(), "Must fail when all candidate providers fail");
}
