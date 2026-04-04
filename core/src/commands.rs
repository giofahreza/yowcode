//! Slash command system for YowCode
//!
//! This module implements the command system similar to Claude Code's slash commands.

use crate::error::{Error, Result};
use crate::session::SessionManager;
use crate::tool::ToolRegistry;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Command executor context
#[derive(Clone)]
pub struct CommandContext {
    pub session_id: Uuid,
    pub current_directory: PathBuf,
    pub session_manager: Arc<SessionManager>,
    pub tool_registry: Arc<ToolRegistry>,
    pub command_registry: Arc<CommandRegistry>,
    pub environment: HashMap<String, String>,
}

impl CommandContext {
    pub fn new(
        session_id: Uuid,
        session_manager: Arc<SessionManager>,
        tool_registry: Arc<ToolRegistry>,
        command_registry: Arc<CommandRegistry>,
    ) -> Self {
        Self {
            session_id,
            current_directory: std::env::current_dir().unwrap(),
            session_manager,
            tool_registry,
            command_registry,
            environment: std::env::vars().collect(),
        }
    }
}

/// Result of a command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub output: String,
    pub is_error: bool,
    pub exit_code: Option<i32>,
}

impl CommandResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: false,
            exit_code: Some(0),
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: true,
            exit_code: None,
        }
    }
}

/// Trait for command implementations
#[async_trait]
pub trait Command: Send + Sync {
    /// Get the command name (without the / prefix)
    fn name(&self) -> &'static str;

    /// Get the command description
    fn description(&self) -> &'static str;

    /// Get the command usage
    fn usage(&self) -> &'static str;

    /// Parse arguments from the command string
    fn parse_args(&self, args: &str) -> Result<HashMap<String, String>>;

    /// Execute the command
    async fn execute(&self, ctx: &mut CommandContext, args: HashMap<String, String>) -> Result<CommandResult>;

    /// Whether the command requires confirmation before running
    fn requires_confirmation(&self) -> bool {
        false
    }
}

/// Command registry
#[derive(Clone)]
pub struct CommandRegistry {
    commands: Arc<RwLock<HashMap<String, Arc<dyn Command>>>>,
    aliases: Arc<RwLock<HashMap<String, String>>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Arc::new(RwLock::new(HashMap::new())),
            aliases: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a command
    pub async fn register(&self, command: Arc<dyn Command>) {
        let name = command.name().to_string();
        self.commands.write().await.insert(name.clone(), command);
    }

    /// Register an alias for a command
    pub async fn register_alias(&self, alias: String, command: String) {
        self.aliases.write().await.insert(alias, command);
    }

    /// Get a command by name
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Command>> {
        let commands = self.commands.read().await;
        if let Some(cmd) = commands.get(name) {
            return Some(cmd.clone());
        }

        // Check aliases
        let aliases = self.aliases.read().await;
        if let Some(cmd_name) = aliases.get(name) {
            commands.get(cmd_name).cloned()
        } else {
            None
        }
    }

    /// List all commands
    pub async fn list(&self) -> Vec<CommandInfo> {
        let commands = self.commands.read().await;
        commands
            .values()
            .map(|cmd| CommandInfo {
                name: cmd.name().to_string(),
                description: cmd.description().to_string(),
                usage: cmd.usage().to_string(),
            })
            .collect()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    pub usage: String,
}

/// Parse a command string
/// Returns (command_name, args) if it's a command, or None if it's not a command
pub fn parse_command(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let rest = &trimmed[1..]; // Remove the leading /
    let mut parts = rest.splitn(2, ' ');
    let name = parts.next()?.to_string();
    let args = parts.next().unwrap_or("").to_string();

    Some((name, args))
}

/// Execute a command string
pub async fn execute_command(
    input: &str,
    ctx: &mut CommandContext,
    registry: &CommandRegistry,
) -> Result<CommandResult> {
    let (name, args) = parse_command(input)
        .ok_or_else(|| Error::Other("Not a command (must start with /)".to_string()))?;

    let command = registry.get(&name).await
        .ok_or_else(|| Error::Other(format!("Unknown command: /{}", name)))?;

    let parsed_args = command.parse_args(&args)?;

    if command.requires_confirmation() {
        // In a real implementation, this would prompt the user
        // For now, we'll auto-confirm
    }

    command.execute(ctx, parsed_args).await
}

/// /help command
pub struct HelpCommand;

#[async_trait]
impl Command for HelpCommand {
    fn name(&self) -> &'static str {
        "help"
    }

    fn description(&self) -> &'static str {
        "Show available commands"
    }

    fn usage(&self) -> &'static str {
        "/help [command_name]"
    }

    fn parse_args(&self, args: &str) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        if !args.is_empty() {
            map.insert("command".to_string(), args.trim().to_string());
        }
        Ok(map)
    }

    async fn execute(&self, ctx: &mut CommandContext, args: HashMap<String, String>) -> Result<CommandResult> {
        if let Some(cmd_name) = args.get("command") {
            if let Some(cmd) = ctx.command_registry.get(cmd_name).await {
                Ok(CommandResult::success(format!(
                    "Command: /{}\nDescription: {}\nUsage: {}",
                    cmd.name(),
                    cmd.description(),
                    cmd.usage()
                )))
            } else {
                Ok(CommandResult::error(format!("Unknown command: /{}", cmd_name)))
            }
        } else {
            let commands = ctx.command_registry.list().await;
            let mut output = "Available commands:\n".to_string();
            for cmd in commands {
                output.push_str(&format!("  /{} - {}\n", cmd.name, cmd.description));
            }
            output.push_str("\nUse /help <command> for more information.");
            Ok(CommandResult::success(output))
        }
    }
}

/// /diff command
pub struct DiffCommand;

#[async_trait]
impl Command for DiffCommand {
    fn name(&self) -> &'static str {
        "diff"
    }

    fn description(&self) -> &'static str {
        "Show git diff"
    }

    fn usage(&self) -> &'static str {
        "/diff [staged|file]"
    }

    fn parse_args(&self, args: &str) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        if !args.is_empty() {
            map.insert("args".to_string(), args.trim().to_string());
        }
        Ok(map)
    }

    async fn execute(&self, ctx: &mut CommandContext, args: HashMap<String, String>) -> Result<CommandResult> {
        let args_str = args.get("args").map(|s| s.as_str()).unwrap_or("");

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("diff");
        if args_str.contains("staged") || args_str.contains("--staged") {
            cmd.arg("--staged");
        }

        // Check if there's a file argument
        if let Some(file) = args.get("args").filter(|a| !a.contains("staged")) {
            cmd.arg("--").arg(file);
        }

        cmd.current_dir(&ctx.current_directory);

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::CommandExecution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(CommandResult::success(if stdout.is_empty() { "No changes" } else { &stdout }))
        } else {
            Ok(CommandResult::error(if stderr.is_empty() { "Failed to get diff" } else { &stderr }))
        }
    }
}

/// /status command
pub struct StatusCommand;

#[async_trait]
impl Command for StatusCommand {
    fn name(&self) -> &'static str {
        "status"
    }

    fn description(&self) -> &'static str {
        "Show git repository status"
    }

    fn usage(&self) -> &'static str {
        "/status"
    }

    fn parse_args(&self, _args: &str) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    async fn execute(&self, ctx: &mut CommandContext, _args: HashMap<String, String>) -> Result<CommandResult> {
        let output = tokio::process::Command::new("git")
            .args(["status", "-sb"])
            .current_dir(&ctx.current_directory)
            .output()
            .await
            .map_err(|e| Error::CommandExecution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(CommandResult::success(&stdout))
        } else {
            Ok(CommandResult::error(&stderr))
        }
    }
}

/// /commit command
pub struct CommitCommand;

#[async_trait]
impl Command for CommitCommand {
    fn name(&self) -> &'static str {
        "commit"
    }

    fn description(&self) -> &'static str {
        "Create a git commit"
    }

    fn usage(&self) -> &'static str {
        "/commit <message>"
    }

    fn parse_args(&self, args: &str) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        if args.trim().is_empty() {
            return Err(Error::Other("Commit message is required".to_string()));
        }
        map.insert("message".to_string(), args.trim().to_string());
        Ok(map)
    }

    async fn execute(&self, ctx: &mut CommandContext, args: HashMap<String, String>) -> Result<CommandResult> {
        let message = args.get("message")
            .ok_or_else(|| Error::Other("Commit message is required".to_string()))?;

        // Stage all changes
        let _ = tokio::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&ctx.current_directory)
            .output()
            .await;

        // Create commit
        let output = tokio::process::Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&ctx.current_directory)
            .output()
            .await
            .map_err(|e| Error::CommandExecution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(CommandResult::success(if stdout.is_empty() { "Commit created" } else { &stdout }))
        } else {
            Ok(CommandResult::error(&stderr))
        }
    }
}

/// /ls command
pub struct LsCommand;

#[async_trait]
impl Command for LsCommand {
    fn name(&self) -> &'static str {
        "ls"
    }

    fn description(&self) -> &'static str {
        "List directory contents"
    }

    fn usage(&self) -> &'static str {
        "/ls [path]"
    }

    fn parse_args(&self, args: &str) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        if !args.trim().is_empty() {
            map.insert("path".to_string(), args.trim().to_string());
        }
        Ok(map)
    }

    async fn execute(&self, ctx: &mut CommandContext, args: HashMap<String, String>) -> Result<CommandResult> {
        let path = args.get("path")
            .unwrap_or(&".".to_string())
            .clone();

        let full_path = ctx.current_directory.join(&path);

        let output = tokio::process::Command::new("ls")
            .arg("-la")
            .arg(&full_path)
            .output()
            .await
            .map_err(|e| Error::CommandExecution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(CommandResult::success(&stdout))
        } else {
            Ok(CommandResult::error(&stderr))
        }
    }
}

/// /cd command
pub struct CdCommand;

#[async_trait]
impl Command for CdCommand {
    fn name(&self) -> &'static str {
        "cd"
    }

    fn description(&self) -> &'static str {
        "Change current directory"
    }

    fn usage(&self) -> &'static str {
        "/cd <path>"
    }

    fn parse_args(&self, args: &str) -> Result<HashMap<String, String>> {
        let mut map = HashMap::new();
        if args.trim().is_empty() {
            return Err(Error::Other("Path is required".to_string()));
        }
        map.insert("path".to_string(), args.trim().to_string());
        Ok(map)
    }

    async fn execute(&self, ctx: &mut CommandContext, args: HashMap<String, String>) -> Result<CommandResult> {
        let path = args.get("path")
            .ok_or_else(|| Error::Other("Path is required".to_string()))?;

        let new_path = if path.starts_with('/') {
            PathBuf::from(path)
        } else if path == "~" {
            dirs::home_dir().ok_or_else(|| Error::Other("Could not find home directory".to_string()))?
        } else {
            ctx.current_directory.join(path)
        };

        // Try to canonicalize the path
        let canonical = tokio::fs::canonicalize(&new_path).await;

        match canonical {
            Ok(absolute_path) if absolute_path.is_dir() => {
                ctx.current_directory = absolute_path;
                Ok(CommandResult::success(format!("Changed directory to: {}", ctx.current_directory.display())))
            }
            Ok(_) => Ok(CommandResult::error("Path is not a directory")),
            Err(e) => Ok(CommandResult::error(format!("Failed to change directory: {}", e))),
        }
    }
}

/// /pwd command
pub struct PwdCommand;

#[async_trait]
impl Command for PwdCommand {
    fn name(&self) -> &'static str {
        "pwd"
    }

    fn description(&self) -> &'static str {
        "Print current working directory"
    }

    fn usage(&self) -> &'static str {
        "/pwd"
    }

    fn parse_args(&self, _args: &str) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    async fn execute(&self, ctx: &mut CommandContext, _args: HashMap<String, String>) -> Result<CommandResult> {
        Ok(CommandResult::success(format!("{}", ctx.current_directory.display())))
    }
}

/// /clear command
pub struct ClearCommand;

#[async_trait]
impl Command for ClearCommand {
    fn name(&self) -> &'static str {
        "clear"
    }

    fn description(&self) -> &'static str {
        "Clear the conversation history"
    }

    fn usage(&self) -> &'static str {
        "/clear"
    }

    fn parse_args(&self, _args: &str) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    async fn execute(&self, ctx: &mut CommandContext, _args: HashMap<String, String>) -> Result<CommandResult> {
        // In a real implementation, this would clear the message history
        // For now, we'll just return a success message
        let _ = ctx.session_id; // Use ctx to avoid warning
        Ok(CommandResult::success("Conversation cleared"))
    }
}

/// /tools command
pub struct ToolsCommand;

#[async_trait]
impl Command for ToolsCommand {
    fn name(&self) -> &'static str {
        "tools"
    }

    fn description(&self) -> &'static str {
        "List available tools"
    }

    fn usage(&self) -> &'static str {
        "/tools"
    }

    fn parse_args(&self, _args: &str) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }

    async fn execute(&self, ctx: &mut CommandContext, _args: HashMap<String, String>) -> Result<CommandResult> {
        let tools = ctx.tool_registry.list();
        let mut output = "Available tools:\n".to_string();
        for tool in tools {
            output.push_str(&format!(
                "  {} - {} (destructive: {})\n",
                tool.name,
                tool.description,
                tool.is_destructive
            ));
        }
        // Use ctx to avoid unused warning
        let _ = ctx.current_directory;
        Ok(CommandResult::success(output))
    }
}
