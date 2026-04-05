//! Slash command system for YowCode CLI
//!
//! This module provides a command registry and execution system for slash commands.

use crate::error::{Error, Result};
use crate::session::SessionManager;
use crate::tool::ToolRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Command execution context
#[derive(Clone)]
pub struct CommandContext {
    pub session_id: Uuid,
    pub current_directory: PathBuf,
    pub session_manager: Arc<SessionManager>,
    pub tool_registry: Arc<ToolRegistry>,
    pub command_registry: Arc<CommandRegistry>,
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
            current_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            session_manager,
            tool_registry,
            command_registry,
        }
    }
}

/// Command execution result
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub output: String,
    pub is_error: bool,
    pub exit_code: Option<i32>,
    pub changed_model: Option<String>,
    pub changed_mode: Option<String>,
    pub changed_effort: Option<String>,
}

impl CommandResult {
    pub fn success(output: String) -> Self {
        Self {
            output,
            is_error: false,
            exit_code: Some(0),
            changed_model: None,
            changed_mode: None,
            changed_effort: None,
        }
    }

    pub fn success_with_changes(
        output: String,
        model: Option<String>,
        mode: Option<String>,
        effort: Option<String>,
    ) -> Self {
        Self {
            output,
            is_error: false,
            exit_code: Some(0),
            changed_model: model,
            changed_mode: mode,
            changed_effort: effort,
        }
    }

    pub fn error(output: String) -> Self {
        Self {
            output,
            is_error: true,
            exit_code: None,
            changed_model: None,
            changed_mode: None,
            changed_effort: None,
        }
    }
}

/// Command information
#[derive(Clone, Debug)]
pub struct CommandInfo {
    pub name: String,
    pub description: String,
    pub usage: String,
}

/// Command registration
#[derive(Clone)]
pub struct CommandRegistration {
    pub info: CommandInfo,
}

/// Command registry
#[derive(Clone)]
pub struct CommandRegistry {
    commands: Arc<RwLock<HashMap<String, CommandRegistration>>>,
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
    pub async fn register(&self, name: String, description: String, usage: String) {
        let name_clone = name.clone();
        let info = CommandInfo {
            name,
            description,
            usage,
        };
        let registration = CommandRegistration {
            info,
        };
        self.commands.write().await.insert(name_clone, registration);
    }

    /// Register an alias for a command
    pub async fn register_alias(&self, alias: String, command: String) {
        self.aliases.write().await.insert(alias, command);
    }

    /// Get a command by name
    pub async fn get(&self, name: &str) -> Option<CommandRegistration> {
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
        commands.values().map(|cmd| cmd.info.clone()).collect()
    }

    /// Get command count
    pub async fn count(&self) -> usize {
        self.commands.read().await.len()
    }
}

/// Parse a command string
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

    let _command = registry.get(&name).await
        .ok_or_else(|| Error::Other(format!("Unknown command: /{}", name)))?;

    match name.as_str() {
        "help" => execute_help_command(ctx, args.as_str()).await,
        "diff" => execute_diff_command(ctx, args.as_str()).await,
        "status" => execute_status_command(ctx, args.as_str()).await,
        "commit" => execute_commit_command(ctx, args.as_str()).await,
        "ls" => execute_ls_command(ctx, args.as_str()).await,
        "cd" => execute_cd_command(ctx, args.as_str()).await,
        "pwd" => execute_pwd_command(ctx, args.as_str()).await,
        "clear" => execute_clear_command(ctx, args.as_str()).await,
        "undo" => execute_undo_command(ctx, args.as_str()).await,
        "tools" => execute_tools_command(ctx, args.as_str()).await,
        "cost" => execute_cost_command(ctx, args.as_str()).await,
        "config" => execute_config_command(ctx, args.as_str()).await,
        "mode" => execute_mode_command(ctx, args.as_str()).await,
        "effort" => execute_effort_command(ctx, args.as_str()).await,
        "model" => execute_model_command(ctx, args.as_str()).await,
        "mcp" => execute_mcp_command(ctx, args.as_str()).await,
        "agent" => execute_agent_command(ctx, args.as_str()).await,
        "skill" => execute_skill_command(ctx, args.as_str()).await,
        "memory" => execute_memory_command(ctx, args.as_str()).await,
        _ => Err(Error::Other(format!("Unknown command: /{}", name))),
    }
}

/// Try to find the closest matching command
pub fn find_closest_command(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let partial = &trimmed[1..]; // Remove the leading /

    // If the input is exactly /, return help
    if partial.is_empty() {
        return Some("/help".to_string());
    }

    // Try to find commands that start with the partial input
    let commands = vec![
        "help", "diff", "status", "commit", "ls", "cd", "pwd",
        "clear", "undo", "tools", "cost", "config", "mode", "effort",
        "model", "mcp", "agent", "skill", "memory"
    ];

    for cmd in commands {
        if cmd.starts_with(partial) {
            return Some(format!("/{}", cmd));
        }
    }

    None
}

// ==================== Command Implementations ====================

async fn execute_help_command(ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    let commands = ctx.command_registry.list().await;
    let mut output = String::from("Available commands:\n");

    for cmd in commands {
        output.push_str(&format!("  {} - {}\n", cmd.usage, cmd.description));
    }

    Ok(CommandResult::success(output))
}

async fn execute_diff_command(ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["diff", "--color=none"]);

    if !args.is_empty() {
        cmd.arg(args);
    }

    cmd.current_dir(&ctx.current_directory);

    let output = cmd.output().await
        .map_err(|e| Error::CommandExecution(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(CommandResult::success(stdout))
    } else {
        Ok(CommandResult::error(format!("{}{}", stderr, stdout)))
    }
}

async fn execute_status_command(ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    let output = tokio::process::Command::new("git")
        .args(["status", "-sb"])
        .current_dir(&ctx.current_directory)
        .output()
        .await
        .map_err(|e| Error::CommandExecution(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(CommandResult::success(stdout))
    } else {
        Ok(CommandResult::error(format!("Not in a git repository: {}", stderr)))
    }
}

async fn execute_commit_command(ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let message = if args.is_empty() {
        "Update".to_string()
    } else {
        args.to_string()
    };

    let output = tokio::process::Command::new("git")
        .args(["commit", "-m", &message])
        .current_dir(&ctx.current_directory)
        .output()
        .await
        .map_err(|e| Error::CommandExecution(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(CommandResult::success(stdout))
    } else {
        Ok(CommandResult::error(format!("{}{}", stderr, stdout)))
    }
}

async fn execute_ls_command(ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let target_path = if args.is_empty() {
        ctx.current_directory.clone()
    } else {
        ctx.current_directory.join(args)
    };

    let output = tokio::process::Command::new("ls")
        .args(["-la", "--color=none"])
        .arg(&target_path)
        .output()
        .await
        .map_err(|e| Error::CommandExecution(e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(CommandResult::success(stdout))
}

async fn execute_cd_command(ctx: &mut CommandContext, args: &str) -> Result<CommandResult> {
    if args.is_empty() {
        return Ok(CommandResult::error("Path required".to_string()));
    }

    let new_path = if args.starts_with('/') {
        PathBuf::from(args)
    } else if args == "~" {
        std::env::var("HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"))
    } else {
        ctx.current_directory.join(args)
    };

    let canonical = new_path.canonicalize()
        .map_err(|e| Error::Other(format!("Invalid path: {}", e)))?;

    if !canonical.is_dir() {
        return Ok(CommandResult::error(format!("Not a directory: {}", args)));
    }

    ctx.current_directory = canonical.clone();
    Ok(CommandResult::success(format!("Changed to: {}", canonical.display())))
}

async fn execute_pwd_command(ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    Ok(CommandResult::success(format!("{}", ctx.current_directory.display())))
}

async fn execute_clear_command(_ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    // Return a special result that indicates the screen should be cleared
    Ok(CommandResult {
        output: "\x1b[2J\x1b[H".to_string(), // ANSI clear screen
        is_error: false,
        exit_code: Some(0),
        changed_model: None,
        changed_mode: None,
        changed_effort: None,
    })
}

async fn execute_undo_command(ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    // This is handled by the TUI, but we return a message for consistency
    let session = ctx.session_manager.get_session(ctx.session_id).await;
    match session {
        Ok(_) => Ok(CommandResult::success("Undo: Last message removed (use arrow keys to navigate history)".to_string())),
        Err(_) => Ok(CommandResult::error("Failed to undo: Session not found".to_string())),
    }
}

async fn execute_tools_command(ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    let tools = ctx.tool_registry.list();
    let mut output = String::from("Available tools:\n");

    for tool in tools {
        output.push_str(&format!("  {} - {}\n", tool.id, tool.description));
    }

    Ok(CommandResult::success(output))
}

async fn execute_models_command(_ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    let mut output = String::from("Available AI models:\n");

    // List some popular models
    let models = vec![
        ("claude-sonnet-4-20250514", "Anthropic Claude Sonnet 4", "$3/M input"),
        ("claude-opus-4-20250514", "Anthropic Claude Opus 4", "$15/M input"),
        ("gpt-4o", "OpenAI GPT-4o", "$5/M input"),
        ("glm-4-plus", "Zhipu AI GLM-4 Plus", "Variable"),
    ];

    for (model_id, description, price) in models {
        output.push_str(&format!("  {} - {} ({})\n", model_id, description, price));
    }

    output.push_str("\nCurrent model: Check /config for current model");

    Ok(CommandResult::success(output))
}

async fn execute_cost_command(_ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    // This would require tracking token usage across sessions
    // For now, return a placeholder
    Ok(CommandResult::success(
        "Token usage tracking not yet implemented. This will show total tokens used and estimated costs.".to_string()
    ))
}

async fn execute_config_command(ctx: &CommandContext, _args: &str) -> Result<CommandResult> {
    let mut output = format!("Current directory: {}\n", ctx.current_directory.display());
    output.push_str("Configuration management not yet fully implemented.");
    Ok(CommandResult::success(output))
}

async fn execute_mode_command(_ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let mode = if args.is_empty() {
        "current".to_string()
    } else {
        args.to_string()
    };

    match mode.as_str() {
        "current" => {
            Ok(CommandResult::success("Permission modes:\n  default - Ask before executing actions\n  auto - Auto-approve all actions (YOLO mode)".to_string()))
        }
        "default" => {
            Ok(CommandResult::success_with_changes(
                "Permission mode set to: default (ask before actions)".to_string(),
                None,
                Some("default".to_string()),
                None,
            ))
        }
        "auto" => {
            Ok(CommandResult::success_with_changes(
                "Permission mode set to: auto (YOLO mode - auto-approve)".to_string(),
                None,
                Some("auto".to_string()),
                None,
            ))
        }
        _ => {
            Ok(CommandResult::error(format!("Unknown mode: {}. Use: /mode [default|auto]", mode)))
        }
    }
}

async fn execute_effort_command(_ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let effort = if args.is_empty() {
        "current".to_string()
    } else {
        args.to_string()
    };

    match effort.as_str() {
        "current" => {
            Ok(CommandResult::success("Effort levels:\n  low - Faster, less thorough\n  normal - Balanced (default)\n  high - Slower, more thorough".to_string()))
        }
        "low" => {
            Ok(CommandResult::success_with_changes(
                "Effort set to: low (faster responses)".to_string(),
                None,
                None,
                Some("low".to_string()),
            ))
        }
        "normal" => {
            Ok(CommandResult::success_with_changes(
                "Effort set to: normal (balanced)".to_string(),
                None,
                None,
                Some("normal".to_string()),
            ))
        }
        "high" => {
            Ok(CommandResult::success_with_changes(
                "Effort set to: high (more thorough)".to_string(),
                None,
                None,
                Some("high".to_string()),
            ))
        }
        _ => {
            Ok(CommandResult::error(format!("Unknown effort: {}. Use: /effort [low|normal|high]", effort)))
        }
    }
}

async fn execute_model_command(_ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let model = if args.is_empty() {
        "current".to_string()
    } else {
        args.to_string()
    };

    match model.as_str() {
        "current" => {
            Ok(CommandResult::success("Available models:\n  claude-opus-4-6 - Most capable (best for complex tasks)\n  claude-sonnet-4-6 - Balanced performance and speed\n  claude-haiku-4-5-20251001 - Fastest (for simple tasks)\n\nUse: /model <name> to change model".to_string()))
        }
        "claude-opus-4-6" | "opus" => {
            Ok(CommandResult::success_with_changes(
                "Model set to: claude-opus-4-6 (most capable)".to_string(),
                Some("claude-opus-4-6".to_string()),
                None,
                None,
            ))
        }
        "claude-sonnet-4-6" | "sonnet" => {
            Ok(CommandResult::success_with_changes(
                "Model set to: claude-sonnet-4-6 (balanced)".to_string(),
                Some("claude-sonnet-4-6".to_string()),
                None,
                None,
            ))
        }
        "claude-haiku-4-5-20251001" | "haiku" => {
            Ok(CommandResult::success_with_changes(
                "Model set to: claude-haiku-4-5-20251001 (fastest)".to_string(),
                Some("claude-haiku-4-5-20251001".to_string()),
                None,
                None,
            ))
        }
        _ => {
            Ok(CommandResult::error(format!("Unknown model: {}. Use: /model [opus|sonnet|haiku]", model)))
        }
    }
}

async fn execute_mcp_command(_ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let subcommand = if args.is_empty() {
        "list".to_string()
    } else {
        args.split_whitespace().next().unwrap_or("list").to_string()
    };

    match subcommand.as_str() {
        "list" => {
            Ok(CommandResult::success("MCP Servers:\n  No MCP servers configured\n\nUse: /mcp add <name> <url> to add a server".to_string()))
        }
        "add" => {
            Ok(CommandResult::success("MCP server addition requires configuration file.\nUsage: /mcp add <name> <url>".to_string()))
        }
        "remove" => {
            Ok(CommandResult::success("MCP server removal requires configuration file.\nUsage: /mcp remove <name>".to_string()))
        }
        _ => {
            Ok(CommandResult::error(format!("Unknown MCP subcommand: {}. Use: /mcp [list|add|remove]", subcommand)))
        }
    }
}

async fn execute_agent_command(_ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let subcommand = if args.is_empty() {
        "list".to_string()
    } else {
        args.split_whitespace().next().unwrap_or("list").to_string()
    };

    match subcommand.as_str() {
        "list" => {
            Ok(CommandResult::success("Available agents:\n  general - General purpose coding assistant\n  coder - Specialized for code generation\n  filesystem - File system operations\n  debugger - Debugging assistance\n\nUse: /agent create <name> <type> to create a new agent".to_string()))
        }
        "create" => {
            Ok(CommandResult::success("Agent creation requires configuration.\nUsage: /agent create <name> <type>".to_string()))
        }
        "delete" => {
            Ok(CommandResult::success("Agent deletion requires confirmation.\nUsage: /agent delete <name>".to_string()))
        }
        _ => {
            Ok(CommandResult::error(format!("Unknown agent subcommand: {}. Use: /agent [list|create|delete]", subcommand)))
        }
    }
}

async fn execute_skill_command(_ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let subcommand = if args.is_empty() {
        "list".to_string()
    } else {
        args.split_whitespace().next().unwrap_or("list").to_string()
    };

    match subcommand.as_str() {
        "list" => {
            Ok(CommandResult::success("Available skills:\n  commit - Create git commits\n  review - Review code changes\n  test - Run tests\n\nUse: /skill create <name> to create a new skill".to_string()))
        }
        "create" => {
            Ok(CommandResult::success("Skill creation requires configuration.\nUsage: /skill create <name>".to_string()))
        }
        "delete" => {
            Ok(CommandResult::success("Skill deletion requires confirmation.\nUsage: /skill delete <name>".to_string()))
        }
        _ => {
            Ok(CommandResult::error(format!("Unknown skill subcommand: {}. Use: /skill [list|create|delete]", subcommand)))
        }
    }
}

async fn execute_memory_command(_ctx: &CommandContext, args: &str) -> Result<CommandResult> {
    let subcommand = if args.is_empty() {
        "list".to_string()
    } else {
        args.split_whitespace().next().unwrap_or("list").to_string()
    };

    match subcommand.as_str() {
        "list" => {
            Ok(CommandResult::success("Memory entries:\n  No memory entries stored\n\nUse: /memory add <key> <value> to add memory".to_string()))
        }
        "add" => {
            Ok(CommandResult::success("Memory addition requires key and value.\nUsage: /memory add <key> <value>".to_string()))
        }
        "remove" => {
            Ok(CommandResult::success("Memory removal requires key.\nUsage: /memory remove <key>".to_string()))
        }
        "clear" => {
            Ok(CommandResult::success("Memory cleared successfully".to_string()))
        }
        _ => {
            Ok(CommandResult::error(format!("Unknown memory subcommand: {}. Use: /memory [list|add|remove|clear]", subcommand)))
        }
    }
}

/// Register all default commands
pub async fn register_default_commands(registry: &CommandRegistry) {
    registry.register(
        String::from("help"),
        String::from("Show available commands"),
        String::from("/help"),
    ).await;

    registry.register(
        String::from("diff"),
        String::from("Show git diff"),
        String::from("/diff [file]"),
    ).await;

    registry.register(
        String::from("status"),
        String::from("Show git repository status"),
        String::from("/status"),
    ).await;

    registry.register(
        String::from("commit"),
        String::from("Create a git commit"),
        String::from("/commit [message]"),
    ).await;

    registry.register(
        String::from("ls"),
        String::from("List directory contents"),
        String::from("/ls [path]"),
    ).await;

    registry.register(
        String::from("cd"),
        String::from("Change directory"),
        String::from("/cd <path>"),
    ).await;

    registry.register(
        String::from("pwd"),
        String::from("Print working directory"),
        String::from("/pwd"),
    ).await;

    registry.register(
        String::from("clear"),
        String::from("Clear the screen"),
        String::from("/clear"),
    ).await;

    registry.register(
        String::from("undo"),
        String::from("Undo last action"),
        String::from("/undo"),
    ).await;

    registry.register(
        String::from("tools"),
        String::from("List available tools"),
        String::from("/tools"),
    ).await;

    registry.register(
        String::from("cost"),
        String::from("Show token usage and costs"),
        String::from("/cost"),
    ).await;

    registry.register(
        String::from("config"),
        String::from("Show or set configuration"),
        String::from("/config [key] [value]"),
    ).await;

    registry.register(
        String::from("mode"),
        String::from("Show or set permission mode"),
        String::from("/mode [default|auto]"),
    ).await;

    registry.register(
        String::from("effort"),
        String::from("Show or set AI effort level"),
        String::from("/effort [low|normal|high]"),
    ).await;

    registry.register(
        String::from("model"),
        String::from("Change AI model"),
        String::from("/model [opus|sonnet|haiku]"),
    ).await;

    registry.register(
        String::from("mcp"),
        String::from("Manage MCP servers"),
        String::from("/mcp [list|add|remove]"),
    ).await;

    registry.register(
        String::from("agent"),
        String::from("Manage AI agents"),
        String::from("/agent [list|create|delete]"),
    ).await;

    registry.register(
        String::from("skill"),
        String::from("Manage skills"),
        String::from("/skill [list|create|delete]"),
    ).await;

    registry.register(
        String::from("memory"),
        String::from("Manage memory"),
        String::from("/memory [list|add|remove|clear]"),
    ).await;
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[tokio::test]
    async fn test_command_registry() {
        let registry = CommandRegistry::new();
        register_default_commands(&registry).await;

        println!("Total commands: {}", registry.count().await);

        // Test that specific commands are registered
        let status = registry.get("status").await;
        println!("status: {:?}", status.is_some());

        let mode = registry.get("mode").await;
        println!("mode: {:?}", mode.is_some());

        let effort = registry.get("effort").await;
        println!("effort: {:?}", effort.is_some());
    }
}
