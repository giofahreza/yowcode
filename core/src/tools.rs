//! Additional tools inspired by Claude Code
//!
//! This module contains tool implementations for various operations
//! beyond the basic file system tools.

use crate::error::{Error, Result};
use crate::tool::{Tool, ToolExecutionContext, ToolExecutor, ToolParameterSchema, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use std::time::Instant;

/// Git commit tool
pub struct CommitTool;

impl CommitTool {
    pub fn definition() -> Tool {
        Tool::new("commit", "Commit", "Create a git commit with staged changes")
            .with_parameter(ToolParameterSchema::new("message", "string", "The commit message"))
            .with_parameter(ToolParameterSchema::new("allow_empty", "boolean", "Allow empty commits").optional())
            .requires_permission(true)
            .is_destructive(true)
    }
}

#[async_trait]
impl ToolExecutor for CommitTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let message = params["message"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'message' parameter".to_string()))?;

        let allow_empty = params["allow_empty"].as_bool().unwrap_or(false);

        let start = Instant::now();

        // Check if we're in a git repository
        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let git_dir = base_dir.join(".git");
        if !git_dir.exists() {
            return Ok(ToolResult::error(
                "commit",
                "Not a git repository (no .git directory found)",
            ));
        }

        // Stage all changes
        let mut add_cmd = tokio::process::Command::new("git");
        add_cmd.args(["add", "-A"])
            .current_dir(&base_dir);

        let add_output = add_cmd
            .output()
            .await
            .map_err(|e| Error::CommandExecution(format!("Failed to stage files: {}", e)))?;

        if !add_output.status.success() {
            return Ok(ToolResult::error(
                "commit",
                format!("Failed to stage files: {}", String::from_utf8_lossy(&add_output.stderr)),
            ));
        }

        // Create commit
        let mut commit_cmd = tokio::process::Command::new("git");
        commit_cmd.args(["commit", "-m", message]);
        if allow_empty {
            commit_cmd.arg("--allow-empty");
        }
        commit_cmd.current_dir(&base_dir);

        let commit_output = commit_cmd
            .output()
            .await
            .map_err(|e| Error::CommandExecution(format!("Failed to create commit: {}", e)))?;

        let duration = start.elapsed().as_millis() as u64;

        if commit_output.status.success() {
            Ok(ToolResult {
                tool_id: "commit".to_string(),
                output: String::from_utf8_lossy(&commit_output.stdout).to_string(),
                is_error: false,
                execution_time_ms: duration,
                metadata: Some(json!({
                    "message": message,
                    "allow_empty": allow_empty,
                })),
            })
        } else {
            Ok(ToolResult {
                tool_id: "commit".to_string(),
                output: String::from_utf8_lossy(&commit_output.stderr).to_string(),
                is_error: true,
                execution_time_ms: duration,
                metadata: None,
            })
        }
    }
}

/// Git diff tool
pub struct DiffTool;

impl DiffTool {
    pub fn definition() -> Tool {
        Tool::new("diff", "Diff", "Show git diff of changes")
            .with_parameter(ToolParameterSchema::new("staged", "boolean", "Show staged changes instead of working directory").optional())
            .with_parameter(ToolParameterSchema::new("file", "string", "Specific file to diff").optional())
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for DiffTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let staged = params["staged"].as_bool().unwrap_or(false);
        let file = params["file"].as_str();

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("diff");
        if staged {
            cmd.arg("--staged");
        }
        if let Some(f) = file {
            cmd.arg("--").arg(f);
        }
        cmd.current_dir(&base_dir);

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::CommandExecution(format!("Failed to get diff: {}", e)))?;

        let duration = start.elapsed().as_millis() as u64;

        let result = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ToolResult {
            tool_id: "diff".to_string(),
            output: if result.is_empty() { stderr } else { result },
            is_error: !output.status.success(),
            execution_time_ms: duration,
            metadata: Some(json!({
                "staged": staged,
                "file": file,
            })),
        })
    }
}

/// Ask user question tool (for interactive prompts)
pub struct AskUserQuestionTool;

impl AskUserQuestionTool {
    pub fn definition() -> Tool {
        Tool::new("ask_user", "Ask User", "Ask the user a question and wait for their response")
            .with_parameter(ToolParameterSchema::new("question", "string", "The question to ask the user"))
            .with_parameter(ToolParameterSchema::new("options", "array", "List of options for the user to choose from").optional())
            .with_parameter(ToolParameterSchema::new("multi_select", "boolean", "Allow multiple selections").optional())
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for AskUserQuestionTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let question = params["question"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'question' parameter".to_string()))?;

        let _options = params["options"].as_array();
        let _multi_select = params["multi_select"].as_bool().unwrap_or(false);

        // In a real implementation, this would prompt the user via the UI
        // For now, we return a placeholder response
        Ok(ToolResult {
            tool_id: "ask_user".to_string(),
            output: format!(
                "Question: {} (Note: Interactive prompts not yet implemented in this mode)",
                question
            ),
            is_error: false,
            execution_time_ms: 0,
            metadata: Some(json!({
                "question": question,
                "options": _options,
                "multi_select": _multi_select,
            })),
        })
    }
}

/// Sleep/delay tool
pub struct SleepTool;

impl SleepTool {
    pub fn definition() -> Tool {
        Tool::new("sleep", "Sleep", "Pause execution for a specified duration")
            .with_parameter(ToolParameterSchema::new("seconds", "number", "Number of seconds to sleep"))
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for SleepTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let seconds = params["seconds"]
            .as_f64()
            .ok_or_else(|| Error::ToolExecution("Missing or invalid 'seconds' parameter".to_string()))?;

        let start = std::time::Instant::now();
        tokio::time::sleep(tokio::time::Duration::from_secs_f64(seconds)).await;
        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "sleep".to_string(),
            output: format!("Slept for {:.2} seconds", seconds),
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "seconds": seconds,
            })),
        })
    }
}

/// Synthetic output tool (for debugging/testing)
pub struct SyntheticOutputTool;

impl SyntheticOutputTool {
    pub fn definition() -> Tool {
        Tool::new("synthetic_output", "Synthetic Output", "Generate synthetic structured output (for testing)")
            .with_parameter(ToolParameterSchema::new("content", "string", "The content to output"))
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for SyntheticOutputTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let content = params["content"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'content' parameter".to_string()))?;

        Ok(ToolResult {
            tool_id: "synthetic_output".to_string(),
            output: content.to_string(),
            is_error: false,
            execution_time_ms: 0,
            metadata: Some(json!({
                "content": content,
                "synthetic": true,
            })),
        })
    }
}

/// Git status tool
pub struct GitStatusTool;

impl GitStatusTool {
    pub fn definition() -> Tool {
        Tool::new("git_status", "Git Status", "Show git working tree status")
            .with_parameter(ToolParameterSchema::new("short", "boolean", "Use short format").optional())
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for GitStatusTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let short = params["short"].as_bool().unwrap_or(false);

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("status");
        if short {
            cmd.arg("--short");
        }
        cmd.current_dir(&base_dir);

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::CommandExecution(format!("Failed to get git status: {}", e)))?;

        let duration = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ToolResult {
            tool_id: "git_status".to_string(),
            output: if stdout.is_empty() { stderr } else { stdout },
            is_error: !output.status.success(),
            execution_time_ms: duration,
            metadata: Some(json!({
                "short": short,
            })),
        })
    }
}

/// Git branch tool
pub struct GitBranchTool;

impl GitBranchTool {
    pub fn definition() -> Tool {
        Tool::new("git_branch", "Git Branch", "List or create git branches")
            .with_parameter(ToolParameterSchema::new("create", "string", "Name of new branch to create").optional())
            .with_parameter(ToolParameterSchema::new("checkout", "string", "Name of branch to checkout").optional())
            .requires_permission(true)
            .is_destructive(true)
    }
}

#[async_trait]
impl ToolExecutor for GitBranchTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let create = params["create"].as_str();
        let checkout = params["checkout"].as_str();

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let mut cmd = tokio::process::Command::new("git");
        cmd.current_dir(&base_dir);

        if let Some(branch_name) = create {
            cmd.args(["branch", branch_name]);
        } else if let Some(branch_name) = checkout {
            cmd.args(["checkout", branch_name]);
        } else {
            cmd.args(["branch", "-a"]);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::CommandExecution(format!("Failed to execute git branch command: {}", e)))?;

        let duration = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ToolResult {
            tool_id: "git_branch".to_string(),
            output: if stdout.is_empty() { stderr } else { stdout },
            is_error: !output.status.success(),
            execution_time_ms: duration,
            metadata: Some(json!({
                "create": create,
                "checkout": checkout,
            })),
        })
    }
}

/// List directory tool
pub struct ListDirectoryTool;

impl ListDirectoryTool {
    pub fn definition() -> Tool {
        Tool::new("ls", "List Directory", "List files and directories")
            .with_parameter(ToolParameterSchema::new("path", "string", "Directory path to list").optional())
            .with_parameter(ToolParameterSchema::new("all", "boolean", "Show hidden files").optional())
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for ListDirectoryTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let path = params["path"].as_str().unwrap_or(".");
        let all = params["all"].as_bool().unwrap_or(false);

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref()
            .map(|d| PathBuf::from(d).join(path))
            .unwrap_or_else(|| PathBuf::from(path));

        let mut entries = Vec::new();

        let mut read_dir = tokio::fs::read_dir(&base_dir).await
            .map_err(|e| Error::IO(e))?;

        while let Some(entry) = read_dir.next_entry().await
            .map_err(|e| Error::IO(e))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().await
                .map_err(|e| Error::IO(e))?;

            if !all && name.starts_with('.') {
                continue;
            }

            let type_marker = if file_type.is_dir() {
                "/"
            } else if file_type.is_symlink() {
                "@"
            } else {
                ""
            };

            entries.push(format!("{}{}", name, type_marker));
        }

        entries.sort();

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "ls".to_string(),
            output: entries.join("\n"),
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "path": path,
                "count": entries.len(),
            })),
        })
    }
}

/// File info tool
pub struct FileInfoTool;

impl FileInfoTool {
    pub fn definition() -> Tool {
        Tool::new("file_info", "File Info", "Get detailed information about a file")
            .with_parameter(ToolParameterSchema::new("path", "string", "The file path"))
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for FileInfoTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'path' parameter".to_string()))?;

        let start = Instant::now();

        let full_path = if let Some(base) = &ctx.current_directory {
            PathBuf::from(base).join(path)
        } else {
            PathBuf::from(path)
        };

        let metadata = tokio::fs::metadata(&full_path)
            .await
            .map_err(|e| Error::IO(e))?;

        let file_type = if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else if metadata.is_symlink() {
            "symlink"
        } else {
            "other"
        };

        let modified = metadata
            .modified()
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
            .unwrap_or(0);

        let output = format!(
            "Path: {}\nType: {}\nSize: {} bytes\nModified: {} (Unix timestamp)",
            full_path.display(),
            file_type,
            metadata.len(),
            modified
        );

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "file_info".to_string(),
            output,
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "path": full_path,
                "type": file_type,
                "size": metadata.len(),
            })),
        })
    }
}
