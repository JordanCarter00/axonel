//! Google Gemini provider adapter for Plexis.

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use crate::error::ProviderError;
use crate::traits::Provider;
use crate::types::{
    ChatMessage, ChatRole, CompletionRequest, CompletionResponse, FinishReason, TokenUsage,
    ToolCall,
};

/// Adapter communicating with Google Gemini REST API.
pub struct GeminiProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl GeminiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }
}

#[derive(Serialize)]
struct GeminiGenerateRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiToolGroup>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generationConfig")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Serialize)]
struct GeminiToolGroup {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "maxOutputTokens")]
    max_output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct GeminiGenerateResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiCandidateContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiCandidateContent {
    parts: Option<Vec<GeminiResponsePart>>,
}

#[derive(Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
    #[serde(rename = "functionCall")]
    function_call: Option<GeminiFunctionCall>,
}

#[derive(Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u32>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u32>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u32>,
}

#[async_trait]
impl Provider for GeminiProvider {
    fn id(&self) -> &str {
        "gemini"
    }

    async fn complete(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let endpoint = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url.trim_end_matches('/'),
            request.model,
            self.api_key
        );

        let mut contents = Vec::new();
        for msg in &request.messages {
            let role = match msg.role {
                ChatRole::System | ChatRole::User => "user".to_string(),
                ChatRole::Assistant => "model".to_string(),
                ChatRole::Tool => "user".to_string(),
            };

            let mut parts = Vec::new();
            if let Some(text) = &msg.content {
                parts.push(GeminiPart::Text { text: text.clone() });
            }
            if let Some(tool_calls) = &msg.tool_calls {
                for call in tool_calls {
                    let args = serde_json::from_str(&call.arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    parts.push(GeminiPart::FunctionCall {
                        function_call: GeminiFunctionCall {
                            name: call.name.clone(),
                            args,
                        },
                    });
                }
            }
            if msg.role == ChatRole::Tool {
                let name = msg
                    .name
                    .clone()
                    .unwrap_or_else(|| "tool_result".to_string());
                let response = msg
                    .content
                    .as_deref()
                    .and_then(|c| serde_json::from_str(c).ok())
                    .unwrap_or_else(|| serde_json::json!({"output": msg.content}));
                parts.push(GeminiPart::FunctionResponse {
                    function_response: GeminiFunctionResponse { name, response },
                });
            }

            if !parts.is_empty() {
                contents.push(GeminiContent { role, parts });
            }
        }

        let tools = if request.tools.is_empty() {
            Vec::new()
        } else {
            let function_declarations = request
                .tools
                .iter()
                .map(|t| GeminiFunctionDeclaration {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                })
                .collect();
            vec![GeminiToolGroup {
                function_declarations,
            }]
        };

        let generation_config = if request.temperature.is_some() || request.max_tokens.is_some() {
            Some(GeminiGenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            })
        } else {
            None
        };

        let payload = GeminiGenerateRequest {
            contents,
            tools,
            generation_config,
        };

        let response = self
            .client
            .post(&endpoint)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return match status {
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    Err(ProviderError::Authentication(error_text))
                }
                StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited {
                    retry_after_secs: None,
                }),
                StatusCode::NOT_FOUND => Err(ProviderError::ModelNotFound(error_text)),
                StatusCode::SERVICE_UNAVAILABLE | StatusCode::BAD_GATEWAY => {
                    Err(ProviderError::Unavailable(error_text))
                }
                _ => Err(ProviderError::ExecutionError(format!(
                    "Gemini API status {}: {}",
                    status, error_text
                ))),
            };
        }

        let parsed: GeminiGenerateResponse = response
            .json()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

        let candidate = parsed
            .candidates
            .and_then(|mut cs| cs.pop())
            .ok_or_else(|| {
                ProviderError::InvalidResponse("No candidate in Gemini response".into())
            })?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        if let Some(content) = candidate.content {
            if let Some(parts) = content.parts {
                for (i, p) in parts.into_iter().enumerate() {
                    if let Some(t) = p.text {
                        text_parts.push(t);
                    }
                    if let Some(call) = p.function_call {
                        let id = format!("call_{}_{}", call.name, i);
                        let args_str = serde_json::to_string(&call.args).unwrap_or_default();
                        tool_calls.push(ToolCall::new(id, call.name, args_str));
                    }
                }
            }
        }

        let finish_reason = match candidate.finish_reason.as_deref() {
            Some("STOP") => {
                if tool_calls.is_empty() {
                    FinishReason::Stop
                } else {
                    FinishReason::ToolCalls
                }
            }
            Some("MAX_TOKENS") => FinishReason::Length,
            Some("SAFETY") => FinishReason::ContentFilter,
            Some(other) => FinishReason::Other(other.to_string()),
            None => {
                if tool_calls.is_empty() {
                    FinishReason::Stop
                } else {
                    FinishReason::ToolCalls
                }
            }
        };

        let usage = parsed
            .usage_metadata
            .map(|u| TokenUsage {
                prompt_tokens: u.prompt_token_count.unwrap_or(0),
                completion_tokens: u.candidates_token_count.unwrap_or(0),
                total_tokens: u.total_token_count.unwrap_or(0),
            })
            .unwrap_or_default();

        let response_text = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        };

        let response_msg = ChatMessage {
            role: ChatRole::Assistant,
            content: response_text,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            tool_call_id: None,
            name: None,
        };

        Ok(CompletionResponse {
            message: response_msg,
            usage,
            finish_reason,
        })
    }
}
