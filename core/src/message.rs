use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A message in a chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: MessageContent,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/// The role of a message sender
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageContent {
    Text(String),
    TextWithImages { text: String, images: Vec<ImageData> },
    ToolCalls(Vec<ToolCall>),
    ToolResult { tool_call_id: String, content: String, is_error: bool },
    Thinking(String),
}

/// Image data for multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageData {
    pub media_type: String,
    pub data: String, // base64 encoded
}

/// A tool call in a message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Chat history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistory {
    pub session_id: Uuid,
    pub messages: Vec<Message>,
    pub total_tokens: u32,
}

impl Message {
    pub fn new(role: MessageRole, content: MessageContent) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content,
            created_at: Utc::now(),
            metadata: None,
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, MessageContent::Text(text.into()))
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self::new(MessageRole::User, MessageContent::Text(text.into()))
    }

    pub fn system(text: impl Into<String>) -> Self {
        Self::new(MessageRole::System, MessageContent::Text(text.into()))
    }

    pub fn tool_calls(calls: Vec<ToolCall>) -> Self {
        Self::new(MessageRole::Assistant, MessageContent::ToolCalls(calls))
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self::new(
            MessageRole::Tool,
            MessageContent::ToolResult {
                tool_call_id: tool_call_id.into(),
                content: content.into(),
                is_error,
            },
        )
    }

    pub fn get_text(&self) -> Option<&String> {
        match &self.content {
            MessageContent::Text(text) => Some(text),
            MessageContent::TextWithImages { text, .. } => Some(text),
            MessageContent::Thinking(text) => Some(text),
            _ => None,
        }
    }
}

impl ChatHistory {
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            messages: Vec::new(),
            total_tokens: 0,
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub fn for_ai(&self) -> Vec<(MessageRole, MessageContent)> {
        self.messages
            .iter()
            .map(|m| (m.role, m.content.clone()))
            .collect()
    }
}
