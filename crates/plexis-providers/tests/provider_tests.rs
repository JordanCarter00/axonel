use plexis_providers::{
    ChatMessage, ChatRole, CompletionRequest, CompletionResponse, FinishReason, Provider,
    ProviderError, ScriptedProvider, TokenUsage, ToolCall, ToolDefinition,
};

#[tokio::test]
async fn test_scripted_provider_basic_completion() {
    let provider = ScriptedProvider::new("test-scripted");
    assert_eq!(provider.id(), "test-scripted");

    let expected = CompletionResponse {
        message: ChatMessage::assistant("I have inspected the project."),
        usage: TokenUsage {
            prompt_tokens: 15,
            completion_tokens: 8,
            total_tokens: 23,
        },
        finish_reason: FinishReason::Stop,
    };
    provider.queue_response(expected.clone());

    let req = CompletionRequest::new("gpt-4o", vec![ChatMessage::user("Hello agent")]);
    let resp = provider.complete(&req).await.expect("complete request");

    assert_eq!(
        resp.message.content.as_deref(),
        Some("I have inspected the project.")
    );
    assert_eq!(resp.finish_reason, FinishReason::Stop);
    assert_eq!(resp.usage.total_tokens, 23);

    // Verify request history recorded
    let history = provider.requests();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].model, "gpt-4o");
    assert_eq!(history[0].messages.len(), 1);
}

#[tokio::test]
async fn test_scripted_provider_tool_calls_sequence() {
    let provider = ScriptedProvider::new("test-scripted");

    let tool_call = ToolCall::new(
        "call_1",
        "filesystem",
        serde_json::json!({
            "action": "write_file",
            "path": "output.txt",
            "content": "Plexis Test"
        })
        .to_string(),
    );

    let tool_resp = CompletionResponse::tool_calls(vec![tool_call.clone()]);
    let final_resp = CompletionResponse::text("Task completed successfully.");

    provider.queue_response(tool_resp);
    provider.queue_response(final_resp);

    // Turn 1: model requests tool invocation
    let req1 = CompletionRequest::new("gpt-4o", vec![ChatMessage::user("Create file")]);
    let turn1 = provider.complete(&req1).await.expect("turn 1");
    assert_eq!(turn1.finish_reason, FinishReason::ToolCalls);
    let calls = turn1.message.tool_calls.expect("tool calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "filesystem");

    // Turn 2: model reports completion after receiving tool result
    let req2 = CompletionRequest::new(
        "gpt-4o",
        vec![
            ChatMessage::user("Create file"),
            ChatMessage::assistant_with_tools(vec![tool_call]),
            ChatMessage::tool_response("call_1", "Successfully wrote 11 bytes"),
        ],
    );
    let turn2 = provider.complete(&req2).await.expect("turn 2");
    assert_eq!(turn2.finish_reason, FinishReason::Stop);
    assert_eq!(
        turn2.message.content.as_deref(),
        Some("Task completed successfully.")
    );
}

#[tokio::test]
async fn test_scripted_provider_error_simulation() {
    let provider = ScriptedProvider::new("test-scripted");
    provider.queue_error("Upstream 503 Service Unavailable");

    let req = CompletionRequest::new("gemini-1.5-pro", vec![ChatMessage::user("Run task")]);
    let err = provider.complete(&req).await.unwrap_err();

    assert!(matches!(err, ProviderError::ExecutionError(msg) if msg.contains("503")));
}

#[test]
fn test_message_and_tool_definition_serialization() {
    let tool = ToolDefinition::new(
        "filesystem",
        "Perform file IO",
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string" }
            }
        }),
    );

    let serialized_tool = serde_json::to_string(&tool).expect("serialize tool");
    assert!(serialized_tool.contains("filesystem"));

    let msg = ChatMessage::system("You are a Plexis autonomous worker.");
    let serialized_msg = serde_json::to_string(&msg).expect("serialize msg");
    assert!(serialized_msg.contains("system"));
    assert_eq!(msg.role, ChatRole::System);
}
