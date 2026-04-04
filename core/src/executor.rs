use crate::ai::{AIClient, ChatCompletionRequest, APIMessage, APIMessageContent, APIToolDefinition, APIFunctionDefinition, StreamCallback};
use crate::error::{Error, Result};
use crate::message::{ChatHistory, Message, MessageContent, MessageRole, ToolCall};
use crate::session::SessionEvent;
use crate::tool::{ToolExecutionContext, ToolPermission, ToolRegistry, ToolResult};
use crate::types::PermissionMode;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Options for execution
#[derive(Clone, Debug)]
pub struct ExecutionOptions {
    pub max_iterations: u32,
    pub max_context_tokens: u32,
    pub permission_mode: PermissionMode,
    pub stream_responses: bool,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            max_context_tokens: 200000,
            permission_mode: PermissionMode::Default,
            stream_responses: true,
        }
    }
}

/// Result of a chat execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub messages: Vec<Message>,
    pub total_tokens: u32,
    pub iteration_count: u32,
    pub duration_ms: u64,
    pub tool_calls: Vec<ToolCall>,
}

/// Executor that runs the chat loop
pub struct ChatExecutor {
    ai_client: Arc<dyn AIClient>,
    tool_registry: Arc<ToolRegistry>,
    session_tx: tokio::sync::broadcast::Sender<SessionEvent>,
    current_directory: Arc<RwLock<Option<String>>>,
}

impl ChatExecutor {
    pub fn new(
        ai_client: Arc<dyn AIClient>,
        tool_registry: Arc<ToolRegistry>,
        session_tx: tokio::sync::broadcast::Sender<SessionEvent>,
    ) -> Self {
        Self {
            ai_client,
            tool_registry,
            session_tx,
            current_directory: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn set_current_directory(&self, path: String) {
        *self.current_directory.write().await = Some(path);
    }

    /// Execute a chat session
    pub async fn execute(
        &self,
        history: &mut ChatHistory,
        query: String,
        options: ExecutionOptions,
        mut on_event: impl FnMut(ExecutionEvent) + Send,
    ) -> Result<ExecutionResult> {
        let start_time = Instant::now();
        let mut iteration_count = 0;
        let mut tool_calls = Vec::new();

        // Add user query to history
        let user_message = Message::user_text(query.clone());
        history.add_message(user_message.clone());

        loop {
            iteration_count += 1;

            if iteration_count > options.max_iterations {
                return Err(Error::Other("Maximum iterations exceeded".to_string()));
            }

            on_event(ExecutionEvent::Iteration { iteration: iteration_count });

            // Build AI request
            let api_messages = self.convert_messages_to_api(&history.messages);

            let request = ChatCompletionRequest {
                model: "claude-sonnet-4-20250514".to_string(),
                messages: api_messages,
                tools: Some(self.build_tool_schema()),
                max_tokens: Some(8192),
                temperature: Some(0.7),
                stream: false,
            };

            on_event(ExecutionEvent::AIThinking);

            // Call AI
            let response = self
                .ai_client
                .chat_completion(request)
                .await
                .map_err(|e| Error::AI(crate::error::AIError::Api(e.to_string())))?;

            if let Some(usage) = response.usage {
                on_event(ExecutionEvent::TokenUsage {
                    prompt: usage.prompt_tokens,
                    completion: usage.completion_tokens,
                    total: usage.total_tokens,
                });
            }

            // Extract response
            if let Some(choice) = response.choices.first() {
                let content = choice.message.content.clone().unwrap_or_default();
                let assistant_message = Message::text(&content);
                history.add_message(assistant_message.clone());

                on_event(ExecutionEvent::AssistantMessage {
                    content: content.clone(),
                });

                // Check for tool calls
                if let Some(calls) = &choice.message.tool_calls {
                    let mut current_tool_calls = Vec::new();

                    for call in calls {
                        let tool_call = ToolCall {
                            id: call.id.clone(),
                            name: call.function.name.clone(),
                            arguments: serde_json::from_str(&call.function.arguments)
                                .unwrap_or_default(),
                        };

                        current_tool_calls.push(tool_call.clone());
                        tool_calls.push(tool_call);
                    }

                    // Execute tool calls
                    for tool_call in current_tool_calls {
                        on_event(ExecutionEvent::ToolCall {
                            tool_name: tool_call.name.clone(),
                        });

                        let result = self
                            .execute_tool_call(
                                &tool_call,
                                &options,
                                &mut |permission| {
                                    on_event(ExecutionEvent::PermissionRequest {
                                        tool_name: tool_call.name.clone(),
                                        description: permission,
                                    });
                                    true // Auto-approve for now
                                },
                            )
                            .await?;

                        let tool_message = Message::tool_result(
                            &tool_call.id,
                            &result.output,
                            result.is_error,
                        );
                        history.add_message(tool_message);

                        on_event(ExecutionEvent::ToolResult {
                            tool_name: tool_call.name.clone(),
                            result: result.output,
                            is_error: result.is_error,
                        });
                    }

                    // Continue loop for tool results
                    continue;
                }
            }

            // No more tool calls, we're done
            break;
        }

        Ok(ExecutionResult {
            messages: history.messages.clone(),
            total_tokens: 0, // Would be accumulated from responses
            iteration_count,
            duration_ms: start_time.elapsed().as_millis() as u64,
            tool_calls,
        })
    }

    async fn execute_tool_call(
        &self,
        tool_call: &ToolCall,
        options: &ExecutionOptions,
        permission_callback: &mut impl FnMut(String) -> bool,
    ) -> Result<ToolResult> {
        let tool = self
            .tool_registry
            .get(&tool_call.name)
            .ok_or_else(|| Error::ToolExecution(format!("Tool not found: {}", tool_call.name)))?;

        let ctx = ToolExecutionContext {
            session_id: Uuid::new_v4(),
            current_directory: self.current_directory.read().await.clone(),
            permission_mode: options.permission_mode,
            environment: std::env::vars().collect(),
        };

        // Check permission
        let permission = tool.check_permission(&ctx, &tool_call.arguments);
        if let ToolPermission::Ask(desc) = permission {
            if !permission_callback(desc) {
                return Ok(ToolResult::error(
                    &tool_call.name,
                    "Permission denied by user",
                ));
            }
        } else if permission == ToolPermission::Deny {
            return Ok(ToolResult::error(&tool_call.name, "Permission denied"));
        }

        // Execute tool
        let start = Instant::now();
        let result = tool.execute(&ctx, tool_call.arguments.clone()).await?;
        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            execution_time_ms: duration,
            ..result
        })
    }

    fn convert_messages_to_api(&self, messages: &[Message]) -> Vec<APIMessage> {
        messages
            .iter()
            .map(|m| {
                let role_str = match m.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                    MessageRole::Tool => "user",
                };

                let content = if let Some(text) = m.get_text() {
                    APIMessageContent::Text(text.clone())
                } else {
                    // For tool results and other content, convert to text
                    APIMessageContent::Text(
                        m.get_text()
                            .cloned()
                            .unwrap_or_else(|| "[Non-text message]".to_string()),
                    )
                };

                APIMessage {
                    role: role_str.to_string(),
                    content,
                }
            })
            .collect()
    }

    fn build_tool_schema(&self) -> Vec<APIToolDefinition> {
        self.tool_registry
            .list()
            .iter()
            .map(|tool| APIToolDefinition {
                tool_type: "function".to_string(),
                function: APIFunctionDefinition {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": tool.parameters.iter().map(|p| {
                            (p.name.clone(), serde_json::json!({
                                "type": p.param_type,
                                "description": p.description
                            }))
                        }).collect::<std::collections::HashMap<_, _>>(),
                        "required": tool.parameters.iter().filter(|p| p.required).map(|p| p.name.clone()).collect::<Vec<_>>()
                    }),
                },
            })
            .collect()
    }
}

/// Events during execution
#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    Iteration { iteration: u32 },
    AIThinking,
    AssistantMessage { content: String },
    ToolCall { tool_name: String },
    ToolResult { tool_name: String, result: String, is_error: bool },
    TokenUsage { prompt: u32, completion: u32, total: u32 },
    PermissionRequest { tool_name: String, description: String },
}
