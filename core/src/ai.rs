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
    Groq,
    Together,
    DeepSeek,
    OpenAICompatible,
    Custom,
}

impl AIProvider {
    /// Get the default base URL for a provider
    pub fn default_base_url(&self) -> &'static str {
        match self {
            AIProvider::OpenAI => "https://api.openai.com/v1/chat/completions",
            AIProvider::Anthropic => "https://api.anthropic.com/v1/messages",
            AIProvider::OpenRouter => "https://openrouter.ai/api/v1/chat/completions",
            AIProvider::Groq => "https://api.groq.com/openai/v1/chat/completions",
            AIProvider::Together => "https://api.together.xyz/v1/chat/completions",
            AIProvider::DeepSeek => "https://api.deepseek.com/v1/chat/completions",
            AIProvider::OpenAICompatible => "https://api.openai.com/v1/chat/completions",
            AIProvider::Custom => "https://api.openai.com/v1/chat/completions",
        }
    }

    /// Get the default model for a provider
    pub fn default_model(&self) -> &'static str {
        match self {
            AIProvider::OpenAI => "gpt-4o-mini",
            AIProvider::Anthropic => "claude-sonnet-4-20250514",
            AIProvider::OpenRouter => "anthropic/claude-sonnet-4",
            AIProvider::Groq => "llama-3.3-70b-versatile",
            AIProvider::Together => "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            AIProvider::DeepSeek => "deepseek-chat",
            AIProvider::OpenAICompatible => "gpt-4o-mini",
            AIProvider::Custom => "gpt-4o-mini",
        }
    }

    /// Check if provider uses Anthropic-style headers
    pub fn uses_anthropic_headers(&self) -> bool {
        matches!(self, AIProvider::Anthropic)
    }

    /// Parse provider from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "openai" => Some(AIProvider::OpenAI),
            "anthropic" | "claude" => Some(AIProvider::Anthropic),
            "openrouter" => Some(AIProvider::OpenRouter),
            "groq" => Some(AIProvider::Groq),
            "together" => Some(AIProvider::Together),
            "deepseek" => Some(AIProvider::DeepSeek),
            "openai-compatible" => Some(AIProvider::OpenAICompatible),
            "custom" => Some(AIProvider::Custom),
            _ => None,
        }
    }
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

/// Model information and capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: AIProvider,
    pub context_length: usize,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub input_cost_per_1k: f64,
    pub output_cost_per_1k: f64,
}

impl ModelInfo {
    /// Create a new model info
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        provider: AIProvider,
        context_length: usize,
    ) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            name: name.into(),
            provider,
            context_length,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
        }
    }

    /// Set tool support
    pub fn with_tools(mut self, supports: bool) -> Self {
        self.supports_tools = supports;
        self
    }

    /// Set vision support
    pub fn with_vision(mut self, supports: bool) -> Self {
        self.supports_vision = supports;
        self
    }

    /// Set streaming support
    pub fn with_streaming(mut self, supports: bool) -> Self {
        self.supports_streaming = supports;
        self
    }

    /// Set pricing
    pub fn with_pricing(mut self, input: f64, output: f64) -> Self {
        self.input_cost_per_1k = input;
        self.output_cost_per_1k = output;
        self
    }
}

/// Model catalog with known models
pub struct ModelCatalog;

impl ModelCatalog {
    /// Get all known models
    pub fn all_models() -> Vec<ModelInfo> {
        vec![
            // OpenAI models
            ModelInfo::new("gpt-4o", "GPT-4o", AIProvider::OpenAI, 128000)
                .with_vision(true)
                .with_pricing(2.5, 10.0),
            ModelInfo::new("gpt-4o-mini", "GPT-4o Mini", AIProvider::OpenAI, 128000)
                .with_vision(true)
                .with_pricing(0.15, 0.6),
            ModelInfo::new("gpt-4-turbo", "GPT-4 Turbo", AIProvider::OpenAI, 128000)
                .with_vision(true)
                .with_pricing(10.0, 30.0),
            ModelInfo::new("gpt-3.5-turbo", "GPT-3.5 Turbo", AIProvider::OpenAI, 16385)
                .with_pricing(0.5, 1.5),

            // Anthropic models
            ModelInfo::new("claude-sonnet-4-20250514", "Claude Sonnet 4", AIProvider::Anthropic, 200000)
                .with_vision(true)
                .with_pricing(3.0, 15.0),
            ModelInfo::new("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet", AIProvider::Anthropic, 200000)
                .with_vision(true)
                .with_pricing(3.0, 15.0),
            ModelInfo::new("claude-3-haiku-20240307", "Claude 3 Haiku", AIProvider::Anthropic, 200000)
                .with_pricing(0.25, 1.25),

            // Groq models
            ModelInfo::new("llama-3.3-70b-versatile", "Llama 3.3 70B", AIProvider::Groq, 128000)
                .with_pricing(0.0, 0.0), // Free tier
            ModelInfo::new("mixtral-8x7b-32768", "Mixtral 8x7b", AIProvider::Groq, 32768)
                .with_pricing(0.0, 0.0),

            // Together models
            ModelInfo::new("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B Turbo", AIProvider::Together, 128000)
                .with_pricing(0.9, 0.9),
            ModelInfo::new("mistralai/Mixtral-8x7B-Instruct-v0.1", "Mixtral 8x7B", AIProvider::Together, 32768)
                .with_pricing(0.3, 0.3),

            // DeepSeek models
            ModelInfo::new("deepseek-chat", "DeepSeek Chat", AIProvider::DeepSeek, 128000)
                .with_pricing(0.0, 0.0), // Very cheap
            ModelInfo::new("deepseek-coder", "DeepSeek Coder", AIProvider::DeepSeek, 128000)
                .with_pricing(0.0, 0.0),

            // GLM models (ZhipuAI)
            ModelInfo::new("glm-4-plus", "GLM-4 Plus", AIProvider::OpenAICompatible, 128000)
                .with_pricing(0.5, 0.5),
            ModelInfo::new("glm-4-flash", "GLM-4 Flash", AIProvider::OpenAICompatible, 128000)
                .with_pricing(0.1, 0.1),
        ]
    }

    /// Get model by ID
    pub fn get_model(id: &str) -> Option<ModelInfo> {
        Self::all_models().into_iter().find(|m| m.id == id)
    }

    /// Get models by provider
    pub fn get_models_by_provider(provider: AIProvider) -> Vec<ModelInfo> {
        Self::all_models()
            .into_iter()
            .filter(|m| m.provider == provider)
            .collect()
    }

    /// Search models by name
    pub fn search_models(query: &str) -> Vec<ModelInfo> {
        let query = query.to_lowercase();
        Self::all_models()
            .into_iter()
            .filter(|m| m.id.to_lowercase().contains(&query) || m.name.to_lowercase().contains(&query))
            .collect()
    }

    /// Get recommended model for a use case
    pub fn recommend(for_coding: bool, for_vision: bool) -> ModelInfo {
        match (for_coding, for_vision) {
            (true, false) => Self::get_model("gpt-4o-mini").unwrap_or(Self::all_models()[0].clone()),
            (true, true) => Self::get_model("gpt-4o").unwrap_or(Self::all_models()[0].clone()),
            (false, false) => Self::get_model("gpt-4o-mini").unwrap_or(Self::all_models()[0].clone()),
            (false, true) => Self::get_model("gpt-4o").unwrap_or(Self::all_models()[0].clone()),
        }
    }
}
