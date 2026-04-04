use crate::error::{AIError, Error, Result};
use crate::message::ToolCall;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use std::time::Duration;

/// AI provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AIProvider {
    OpenAI,
    Anthropic,
    OpenRouter,
    Custom,
}

/// Message format for API calls
#[derive(Debug, Clone, Serialize)]
pub struct APIMessage {
    pub role: String,
    pub content: APIMessageContent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum APIMessageContent {
    Text(String),
    MultiPart(Vec<APIContentPart>),
}

#[derive(Debug, Clone, Serialize)]
pub struct APIContentPart {
    #[serde(rename = "type")]
    pub part_type: String,
    pub text: Option<String>,
    pub source: Option<APIImageSource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct APIImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

/// Tool definition for API
#[derive(Debug, Clone, Serialize)]
pub struct APIToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: APIFunctionDefinition,
}

#[derive(Debug, Clone, Serialize)]
pub struct APIFunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Response from chat completion
#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ResponseMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCallData>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolCallData {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Stream event
#[derive(Debug, Clone, Deserialize)]
pub struct AIStreamEvent {
    pub delta: Option<StreamDelta>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamDelta {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCallData>>,
}

/// AI client configuration
#[derive(Debug, Clone)]
pub struct AIConfig {
    pub provider: AIProvider,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_retries: u32,
    pub timeout: Duration,
}

impl Default for AIConfig {
    fn default() -> Self {
        Self {
            provider: AIProvider::Anthropic,
            api_key: String::new(),
            base_url: "https://api.anthropic.com/v1/messages".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_retries: 3,
            timeout: Duration::from_secs(120),
        }
    }
}

/// Request for chat completion (internal)
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<APIMessage>,
    pub tools: Option<Vec<APIToolDefinition>>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

/// Callback for stream events
pub type StreamCallback = Box<dyn FnMut(AIStreamEvent) -> Result<()> + Send>;

/// Trait for AI client implementations (object-safe)
#[async_trait]
pub trait AIClient: Send + Sync {
    async fn chat_completion(&self, request: ChatCompletionRequest) -> Result<ChatCompletionResponse>;

    async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AIStreamEvent>> + Send>>>;

    async fn stream_chat_completion(
        &self,
        request: ChatCompletionRequest,
        callback: StreamCallback,
    ) -> Result<ChatCompletionResponse>;
}

/// Standard OpenAI-compatible client
pub struct OpenAICompatClient {
    config: AIConfig,
    client: Client,
}

impl OpenAICompatClient {
    pub fn new(config: AIConfig) -> Self {
        let timeout = config.timeout;
        Self {
            config,
            client: Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl AIClient for OpenAICompatClient {
    async fn chat_completion(&self, request: ChatCompletionRequest) -> Result<ChatCompletionResponse> {
        let mut req_builder = self.client.post(&self.config.base_url);

        // Add headers based on provider
        match self.config.provider {
            AIProvider::Anthropic => {
                req_builder = req_builder
                    .header("x-api-key", &self.config.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("anthropic-dangerous-direct-browser-access", "true");
            }
            _ => {
                req_builder = req_builder.header("Authorization", format!("Bearer {}", self.config.api_key));
            }
        }

        let response = req_builder
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::AI(AIError::Api(format!(
                "API request failed: {} - {}",
                status, error_text
            ))));
        }

        let api_response: ChatCompletionResponse = response.json().await?;

        Ok(api_response)
    }

    async fn chat_completion_stream(
        &self,
        _request: ChatCompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AIStreamEvent>> + Send>>> {
        // Simplified stream implementation
        // In a full implementation, this would use SSE (Server-Sent Events)
        let stream = futures::stream::once(async {
            Ok(AIStreamEvent {
                delta: Some(StreamDelta {
                    content: Some("Streamed content".to_string()),
                    tool_calls: None,
                }),
                finish_reason: Some("stop".to_string()),
            })
        });

        Ok(Box::pin(stream))
    }

    async fn stream_chat_completion(
        &self,
        request: ChatCompletionRequest,
        mut callback: StreamCallback,
    ) -> Result<ChatCompletionResponse> {
        let mut stream = self.chat_completion_stream(request).await?;
        let mut full_content = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut finish_reason = None;

        while let Some(event) = stream.next().await {
            let event = event?;
            finish_reason = event.finish_reason.clone();

            if let Some(delta) = event.delta.clone() {
                if let Some(content) = &delta.content {
                    full_content.push_str(content);
                    callback(event)?;
                }

                if let Some(calls) = &delta.tool_calls {
                    for call in calls {
                        tool_calls.push(ToolCall {
                            id: call.id.clone(),
                            name: call.function.name.clone(),
                            arguments: serde_json::from_str(&call.function.arguments)
                                .unwrap_or(json!({})),
                        });
                    }
                }
            }
        }

        Ok(ChatCompletionResponse {
            id: uuid::Uuid::new_v4().to_string(),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            model: "unknown".to_string(),
            choices: vec![Choice {
                index: 0,
                message: ResponseMessage {
                    role: "assistant".to_string(),
                    content: Some(full_content),
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls.iter().map(|tc| ToolCallData {
                            id: tc.id.clone(),
                            call_type: Some("function".to_string()),
                            function: FunctionCall {
                                name: tc.name.clone(),
                                arguments: serde_json::to_string(&tc.arguments).unwrap(),
                            },
                        }).collect())
                    },
                },
                finish_reason,
            }],
            usage: None,
        })
    }
}
