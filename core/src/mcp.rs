//! Model Context Protocol (MCP) integration for YowCode
//!
//! This module provides MCP server/client integration that allows:
//! - Connecting to external MCP servers for additional capabilities
//! - Exposing local tools via MCP protocol
//! - Standardized tool/message passing

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// MCP message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MCPMessage {
    /// Request from client to server
    Request { id: String, method: String, params: serde_json::Value },
    /// Response from server to client
    Response { id: String, result: Option<serde_json::Value>, error: Option<MCPError> },
    /// Notification from server to client
    Notification { method: String, params: serde_json::Value },
}

/// MCP error format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// MCP tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// MCP resource definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPResource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerConfig {
    pub id: Uuid,
    pub name: String,
    pub endpoint: String,
    pub transport: MCPTransport,
    pub capabilities: MCPCapabilities,
    pub enabled: bool,
}

/// MCP transport types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MCPTransport {
    Stdio,
    SSE,
    WebSocket,
}

/// MCP server capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPCapabilities {
    pub tools: bool,
    pub resources: bool,
    pub prompts: bool,
    pub logging: bool,
}

impl Default for MCPCapabilities {
    fn default() -> Self {
        Self {
            tools: true,
            resources: false,
            prompts: false,
            logging: false,
        }
    }
}

/// MCP connection state
#[derive(Debug, Clone)]
pub struct MCPConnection {
    pub config: MCPServerConfig,
    pub connected: bool,
    pub tools: Vec<MCPTool>,
    pub resources: Vec<MCPResource>,
}

/// MCP client for connecting to external servers
#[derive(Debug, Clone)]
pub struct MCPClient {
    servers: HashMap<Uuid, MCPConnection>,
}

impl Default for MCPClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MCPClient {
    /// Create a new MCP client
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
        }
    }

    /// Add a server configuration
    pub fn add_server(&mut self, config: MCPServerConfig) -> Result<()> {
        let connection = MCPConnection {
            config: config.clone(),
            connected: false,
            tools: Vec::new(),
            resources: Vec::new(),
        };
        self.servers.insert(config.id, connection);
        Ok(())
    }

    /// Connect to a server
    pub async fn connect(&mut self, server_id: Uuid) -> Result<()> {
        if let Some(connection) = self.servers.get_mut(&server_id) {
            // TODO: Implement actual connection logic based on transport type
            connection.connected = true;
            Ok(())
        } else {
            Err(Error::Other(format!("Server not found: {}", server_id)))
        }
    }

    /// Disconnect from a server
    pub async fn disconnect(&mut self, server_id: Uuid) -> Result<()> {
        if let Some(connection) = self.servers.get_mut(&server_id) {
            connection.connected = false;
            Ok(())
        } else {
            Err(Error::Other(format!("Server not found: {}", server_id)))
        }
    }

    /// List all servers
    pub fn list_servers(&self) -> Vec<&MCPServerConfig> {
        self.servers.values().map(|c| &c.config).collect()
    }

    /// Get tools from a server
    pub fn get_server_tools(&self, server_id: Uuid) -> Result<Vec<MCPTool>> {
        self.servers
            .get(&server_id)
            .map(|c| c.tools.clone())
            .ok_or_else(|| Error::Other(format!("Server not found: {}", server_id)))
    }

    /// Call a tool on a server
    pub async fn call_tool(
        &mut self,
        server_id: Uuid,
        _tool_name: String,
        _arguments: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        if let Some(_connection) = self.servers.get(&server_id) {
            // TODO: Implement actual tool calling logic
            Ok(serde_json::json!({"result": "Tool called"}))
        } else {
            Err(Error::Other(format!("Server not found: {}", server_id)))
        }
    }
}

/// MCP server for exposing local capabilities
#[derive(Debug)]
pub struct MCPServer {
    pub config: MCPServerConfig,
    tools: Vec<MCPTool>,
}

impl MCPServer {
    /// Create a new MCP server
    pub fn new(name: String, transport: MCPTransport) -> Self {
        let config = MCPServerConfig {
            id: Uuid::new_v4(),
            name,
            endpoint: String::new(),
            transport,
            capabilities: MCPCapabilities::default(),
            enabled: true,
        };

        Self {
            config,
            tools: Vec::new(),
        }
    }

    /// Add a tool to the server
    pub fn add_tool(&mut self, tool: MCPTool) {
        self.tools.push(tool);
    }

    /// List available tools
    pub fn list_tools(&self) -> Vec<&MCPTool> {
        self.tools.iter().collect()
    }

    /// Handle an incoming MCP message
    pub async fn handle_message(&self, message: MCPMessage) -> Option<MCPMessage> {
        match message {
            MCPMessage::Request { id, method, params: _ } => {
                match method.as_str() {
                    "tools/list" => {
                        let tools = serde_json::to_value(&self.tools).ok()?;
                        Some(MCPMessage::Response {
                            id,
                            result: Some(tools),
                            error: None,
                        })
                    }
                    _ => Some(MCPMessage::Response {
                        id,
                        result: None,
                        error: Some(MCPError {
                            code: -32601,
                            message: format!("Method not found: {}", method),
                            data: None,
                        }),
                    }),
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_client_creation() {
        let client = MCPClient::new();
        assert!(client.list_servers().is_empty());
    }

    #[test]
    fn test_mcp_server_creation() {
        let server = MCPServer::new("test_server".to_string(), MCPTransport::Stdio);
        assert_eq!(server.config.name, "test_server");
        assert_eq!(server.config.transport, MCPTransport::Stdio);
    }
}
