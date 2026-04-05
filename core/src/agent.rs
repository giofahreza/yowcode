//! Agent system for YowCode
//!
//! This module provides a flexible agent system that allows:
//! - Creating custom agents with specific capabilities
//! - Agent chaining and delegation
//! - Specialized agent types for different tasks

// use crate::error::{Error, Result};  // Not currently used
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentType::General => write!(f, "General"),
            AgentType::Coder => write!(f, "Coder"),
            AgentType::FileSystem => write!(f, "FileSystem"),
            AgentType::Debugger => write!(f, "Debugger"),
            AgentType::Web => write!(f, "Web"),
            AgentType::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

/// Agent capability descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapability {
    pub name: String,
    pub description: String,
    pub enabled: bool,
}

/// Agent instance
#[derive(Debug, Clone)]
pub struct Agent {
    pub config: AgentConfig,
}

impl Agent {
    /// Create a new agent from configuration
    pub fn new(config: AgentConfig) -> Self {
        Self { config }
    }

    /// Get the agent's system prompt
    pub fn system_prompt(&self) -> &str {
        &self.config.system_prompt
    }

    /// Get the agent's tools
    pub fn tools(&self) -> &[String] {
        &self.config.tools
    }
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub agent_type: AgentType,
    pub capabilities: Vec<AgentCapability>,
    pub system_prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub tools: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// Predefined agent types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentType {
    /// General purpose assistant
    General,
    /// Code-focused agent
    Coder,
    /// File system operations
    FileSystem,
    /// Debug and troubleshooting
    Debugger,
    /// Web search and fetch
    Web,
    /// Custom user-defined agent
    Custom(String),
}

impl AgentType {
    pub fn default_system_prompt(&self) -> String {
        match self {
            AgentType::General => {
                "You are a helpful AI assistant with access to various tools. \
                Help the user accomplish their tasks efficiently and accurately."
                    .to_string()
            }
            AgentType::Coder => {
                "You are an expert programmer and code reviewer. \
                Focus on writing clean, efficient, and well-documented code. \
                Always consider best practices, performance, and maintainability."
                    .to_string()
            }
            AgentType::FileSystem => {
                "You are a file system specialist. \
                Help users navigate, organize, and manipulate files and directories. \
                Always be careful with destructive operations."
                    .to_string()
            }
            AgentType::Debugger => {
                "You are a debugging expert. \
                Help identify, diagnose, and fix issues in code. \
                Think step by step and consider edge cases."
                    .to_string()
            }
            AgentType::Web => {
                "You are a web research specialist. \
                Help users find information online and summarize findings."
                    .to_string()
            }
            AgentType::Custom(name) => {
                format!("You are a specialized agent: {}", name)
            }
        }
    }

    pub fn default_tools(&self) -> Vec<String> {
        match self {
            AgentType::General => vec![
                "bash".to_string(),
                "read".to_string(),
                "write".to_string(),
                "edit".to_string(),
                "glob".to_string(),
                "grep".to_string(),
            ],
            AgentType::Coder => vec![
                "bash".to_string(),
                "read".to_string(),
                "write".to_string(),
                "edit".to_string(),
                "glob".to_string(),
                "grep".to_string(),
                "git_status".to_string(),
                "git_branch".to_string(),
                "commit".to_string(),
            ],
            AgentType::FileSystem => vec![
                "bash".to_string(),
                "read".to_string(),
                "write".to_string(),
                "list_directory".to_string(),
                "file_info".to_string(),
                "glob".to_string(),
            ],
            AgentType::Debugger => vec![
                "bash".to_string(),
                "read".to_string(),
                "grep".to_string(),
                "glob".to_string(),
                "file_info".to_string(),
            ],
            AgentType::Web => vec![
                "bash".to_string(),
                "web_fetch".to_string(),
                "web_search".to_string(),
                "read".to_string(),
                "write".to_string(),
            ],
            AgentType::Custom(_) => vec![],
        }
    }
}

impl AgentConfig {
    /// Create a new agent configuration
    pub fn new(name: String, agent_type: AgentType) -> Self {
        let id = Uuid::new_v4();
        let system_prompt = agent_type.default_system_prompt();
        let tools = agent_type.default_tools();

        Self {
            id,
            name,
            description: format!("{} agent", agent_type),
            agent_type,
            capabilities: Vec::new(),
            system_prompt,
            temperature: 0.7,
            max_tokens: 4096,
            tools,
            metadata: HashMap::new(),
        }
    }

    /// Add a capability to the agent
    pub fn with_capability(mut self, name: String, description: String) -> Self {
        self.capabilities.push(AgentCapability {
            name,
            description,
            enabled: true,
        });
        self
    }

    /// Set a custom system prompt
    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = prompt;
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Add a tool to the agent
    pub fn with_tool(mut self, tool: String) -> Self {
        self.tools.push(tool);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Agent registry for managing available agents
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    agents: HashMap<Uuid, AgentConfig>,
    by_name: HashMap<String, Uuid>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRegistry {
    /// Create a new agent registry
    pub fn new() -> Self {
        let mut registry = Self {
            agents: HashMap::new(),
            by_name: HashMap::new(),
        };

        // Register default agents
        registry.register_default_agents();
        registry
    }

    /// Register the default agents
    fn register_default_agents(&mut self) {
        let general = AgentConfig::new("General".to_string(), AgentType::General);
        self.register(general);

        let coder = AgentConfig::new("Coder".to_string(), AgentType::Coder);
        self.register(coder);

        let debugger = AgentConfig::new("Debugger".to_string(), AgentType::Debugger);
        self.register(debugger);
    }

    /// Register an agent
    pub fn register(&mut self, agent: AgentConfig) {
        let id = agent.id;
        let name = agent.name.clone();
        self.agents.insert(id, agent);
        self.by_name.insert(name, id);
    }

    /// Get an agent by ID
    pub fn get(&self, id: Uuid) -> Option<&AgentConfig> {
        self.agents.get(&id)
    }

    /// Get an agent by name
    pub fn get_by_name(&self, name: &str) -> Option<&AgentConfig> {
        self.by_name.get(name).and_then(|id| self.agents.get(id))
    }

    /// List all agents
    pub fn list(&self) -> Vec<&AgentConfig> {
        self.agents.values().collect()
    }

    /// Remove an agent
    pub fn remove(&mut self, id: Uuid) -> Option<AgentConfig> {
        let agent = self.agents.remove(&id)?;
        self.by_name.remove(&agent.name);
        Some(agent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_config_creation() {
        let agent = AgentConfig::new("TestAgent".to_string(), AgentType::Coder);
        assert_eq!(agent.name, "TestAgent");
        assert_eq!(agent.agent_type, AgentType::Coder);
        assert!(!agent.tools.is_empty());
    }

    #[test]
    fn test_agent_registry() {
        let mut registry = AgentRegistry::new();
        assert_eq!(registry.list().len(), 3); // Default agents

        let custom = AgentConfig::new("Custom".to_string(), AgentType::Custom("Special".to_string()));
        let id = custom.id;
        registry.register(custom);

        assert!(registry.get(id).is_some());
        assert!(registry.get_by_name("Custom").is_some());
    }
}
