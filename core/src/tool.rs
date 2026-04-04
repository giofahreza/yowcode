use crate::error::{Error, Result};
use crate::types::PermissionMode;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Unique identifier for a tool
pub type ToolId = String;

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_id: ToolId,
    pub output: String,
    pub is_error: bool,
    pub execution_time_ms: u64,
    pub metadata: Option<Value>,
}

impl ToolResult {
    pub fn success(tool_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            tool_id: tool_id.into(),
            output: output.into(),
            is_error: false,
            execution_time_ms: 0,
            metadata: None,
        }
    }

    pub fn error(tool_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            tool_id: tool_id.into(),
            output: error.into(),
            is_error: true,
            execution_time_ms: 0,
            metadata: None,
        }
    }
}

/// Permission check result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPermission {
    Allow,
    Deny,
    Ask(String), // Ask with a description
}

/// Execution context for tools
#[derive(Clone)]
pub struct ToolExecutionContext {
    pub session_id: Uuid,
    pub current_directory: Option<String>,
    pub permission_mode: PermissionMode,
    pub environment: HashMap<String, String>,
}

impl Default for ToolExecutionContext {
    fn default() -> Self {
        Self {
            session_id: Uuid::new_v4(),
            current_directory: None,
            permission_mode: PermissionMode::Default,
            environment: HashMap::new(),
        }
    }
}

/// Schema for tool input parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameterSchema {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
}

impl ToolParameterSchema {
    pub fn new(name: impl Into<String>, param_type: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param_type: param_type.into(),
            description: description.into(),
            required: true,
            default: None,
        }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn with_default(mut self, default: Value) -> Self {
        self.default = Some(default);
        self
    }
}

/// Definition of a tool
#[derive(Clone)]
pub struct Tool {
    pub id: ToolId,
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParameterSchema>,
    pub requires_permission: bool,
    pub is_destructive: bool,
}

impl Tool {
    pub fn new(id: impl Into<String>, name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            parameters: Vec::new(),
            requires_permission: true,
            is_destructive: false,
        }
    }

    pub fn with_parameter(mut self, param: ToolParameterSchema) -> Self {
        self.parameters.push(param);
        self
    }

    pub fn requires_permission(mut self, requires: bool) -> Self {
        self.requires_permission = requires;
        self
    }

    pub fn is_destructive(mut self, destructive: bool) -> Self {
        self.is_destructive = destructive;
        self
    }
}

/// Trait for tool implementations
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Get the tool definition
    fn definition(&self) -> &Tool;

    /// Check if this tool can be executed with the given parameters
    fn can_execute(&self, params: &Value) -> bool {
        for param in &self.definition().parameters {
            if param.required && !params.get(&param.name).is_some() {
                return false;
            }
        }
        true
    }

    /// Validate and parse parameters
    fn validate_params(&self, params: &Value) -> Result<()> {
        for param in &self.definition().parameters {
            if param.required {
                params.get(&param.name)
                    .ok_or_else(|| Error::ToolExecution(format!("Missing required parameter: {}", param.name)))?;
            }
        }
        Ok(())
    }

    /// Execute the tool with the given parameters
    async fn execute(&self, ctx: &ToolExecutionContext, params: Value) -> Result<ToolResult>;

    /// Get a permission check for this execution
    fn check_permission(&self, ctx: &ToolExecutionContext, _params: &Value) -> ToolPermission {
        match ctx.permission_mode {
            PermissionMode::Bypass => ToolPermission::Allow,
            PermissionMode::Auto if !self.definition().is_destructive => ToolPermission::Allow,
            PermissionMode::Auto => ToolPermission::Ask(format!(
                "Execute destructive action: {}",
                self.definition().name
            )),
            PermissionMode::AlwaysAsk => ToolPermission::Ask(format!(
                "Execute tool: {}",
                self.definition().name
            )),
            PermissionMode::Default | PermissionMode::Plan => {
                if self.definition().is_destructive {
                    ToolPermission::Ask(format!(
                        "Execute destructive action: {}",
                        self.definition().name
                    ))
                } else {
                    ToolPermission::Allow
                }
            }
        }
    }
}

/// Registry for all available tools
#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<ToolId, Arc<dyn ToolExecutor>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn ToolExecutor>) {
        let id = tool.definition().id.clone();
        self.tools.insert(id, tool);
    }

    /// Get a tool by ID
    pub fn get(&self, id: &str) -> Option<&Arc<dyn ToolExecutor>> {
        self.tools.get(id)
    }

    /// List all tools
    pub fn list(&self) -> Vec<&Tool> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Find tools by name pattern
    pub fn find(&self, pattern: &str) -> Vec<&Tool> {
        self.tools
            .values()
            .filter(|t| t.definition().name.to_lowercase().contains(&pattern.to_lowercase()))
            .map(|t| t.definition())
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result() {
        let success = ToolResult::success("test", "output");
        assert!(!success.is_error);
        assert_eq!(success.tool_id, "test");

        let error = ToolResult::error("test", "error");
        assert!(error.is_error);
    }

    #[test]
    fn test_tool_creation() {
        let tool = Tool::new("read_file", "Read File", "Read a file from the filesystem")
            .with_parameter(ToolParameterSchema::new("path", "string", "The file path to read").optional())
            .requires_permission(false);

        assert_eq!(tool.id, "read_file");
        assert_eq!(tool.parameters.len(), 1);
        assert!(!tool.requires_permission);
    }
}
