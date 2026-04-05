//! Additional tools inspired by Claude Code
//!
//! This module contains tool implementations for various operations
//! including file system operations, git commands, and web tools.

use crate::error::{Error, Result};
use crate::tool::{Tool, ToolExecutionContext, ToolExecutor, ToolParameterSchema, ToolResult};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::path::PathBuf;
use std::time::Instant;

// ==================== Basic File System Tools ====================

/// Bash command execution tool
pub struct BashTool;

impl BashTool {
    pub fn definition() -> Tool {
        Tool::new("bash", "Bash", "Execute a bash command")
            .with_parameter(ToolParameterSchema::new("command", "string", "The bash command to execute"))
            .with_parameter(ToolParameterSchema::new("timeout", "number", "Timeout in seconds").optional())
            .requires_permission(true)
            .is_destructive(true)
    }
}

#[async_trait]
impl ToolExecutor for BashTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'command' parameter".to_string()))?;

        let timeout = params["timeout"].as_u64().unwrap_or(30);

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let output = tokio::time::timeout(
            tokio::time::Duration::from_secs(timeout),
            tokio::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .current_dir(&base_dir)
                .output(),
        )
        .await
        .map_err(|_| Error::ToolExecution(format!("Command timed out after {} seconds", timeout)))?
        .map_err(|e| Error::ToolExecution(format!("Failed to execute command: {}", e)))?;

        let duration = start.elapsed().as_millis() as u64;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ToolResult {
            tool_id: "bash".to_string(),
            output: if output.status.success() { stdout } else { stderr },
            is_error: !output.status.success(),
            execution_time_ms: duration,
            metadata: Some(json!({
                "command": command,
                "exit_code": output.status.code(),
            })),
        })
    }
}

/// Read file tool
pub struct ReadTool;

impl ReadTool {
    pub fn definition() -> Tool {
        Tool::new("read_file", "Read File", "Read a file from the filesystem")
            .with_parameter(ToolParameterSchema::new("path", "string", "Path to the file to read"))
            .with_parameter(ToolParameterSchema::new("offset", "number", "Offset in bytes").optional())
            .with_parameter(ToolParameterSchema::new("limit", "number", "Limit in bytes").optional())
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for ReadTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'path' parameter".to_string()))?;

        let offset = params["offset"].as_u64().unwrap_or(0);
        let limit = params["limit"].as_u64();

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let full_path = if path.starts_with('/') {
            PathBuf::from(path)
        } else {
            base_dir.join(path)
        };

        let content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| Error::ToolExecution(format!("Failed to read file: {}", e)))?;

        let content: String = if let Some(limit) = limit {
            let bytes = content.as_bytes();
            let offset = offset as usize;
            let limit = limit as usize;
            if offset < bytes.len() {
                String::from_utf8_lossy(&bytes[offset..offset.min(offset + limit)]).to_string()
            } else {
                String::new()
            }
        } else if offset > 0 {
            let bytes = content.as_bytes();
            String::from_utf8_lossy(bytes.get(offset as usize..).unwrap_or(&[])).to_string()
        } else {
            content
        };

        let duration = start.elapsed().as_millis() as u64;
        let size = content.len();

        Ok(ToolResult {
            tool_id: "read_file".to_string(),
            output: content,
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "path": full_path.display().to_string(),
                "size": size,
            })),
        })
    }
}

/// Write file tool
pub struct WriteTool;

impl WriteTool {
    pub fn definition() -> Tool {
        Tool::new("write_file", "Write File", "Write content to a file")
            .with_parameter(ToolParameterSchema::new("path", "string", "Path to the file to write"))
            .with_parameter(ToolParameterSchema::new("content", "string", "Content to write"))
            .requires_permission(true)
            .is_destructive(true)
    }
}

#[async_trait]
impl ToolExecutor for WriteTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'path' parameter".to_string()))?;

        let content = params["content"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'content' parameter".to_string()))?;

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let full_path = if path.starts_with('/') {
            PathBuf::from(path)
        } else {
            base_dir.join(path)
        };

        // Create parent directories if they don't exist
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::ToolExecution(format!("Failed to create directories: {}", e)))?;
        }

        tokio::fs::write(&full_path, content)
            .await
            .map_err(|e| Error::ToolExecution(format!("Failed to write file: {}", e)))?;

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "write_file".to_string(),
            output: format!("Successfully wrote {} bytes to {}", content.len(), full_path.display()),
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "path": full_path.display().to_string(),
                "size": content.len(),
            })),
        })
    }
}

/// Edit file tool (replace text)
pub struct EditTool;

impl EditTool {
    pub fn definition() -> Tool {
        Tool::new("edit_file", "Edit File", "Edit a file by replacing text")
            .with_parameter(ToolParameterSchema::new("path", "string", "Path to the file to edit"))
            .with_parameter(ToolParameterSchema::new("old_string", "string", "Text to replace"))
            .with_parameter(ToolParameterSchema::new("new_string", "string", "New text to insert"))
            .with_parameter(ToolParameterSchema::new("replace_all", "boolean", "Replace all occurrences").optional())
            .requires_permission(true)
            .is_destructive(true)
    }
}

#[async_trait]
impl ToolExecutor for EditTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'path' parameter".to_string()))?;

        let old_string = params["old_string"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'old_string' parameter".to_string()))?;

        let new_string = params["new_string"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'new_string' parameter".to_string()))?;

        let replace_all = params["replace_all"].as_bool().unwrap_or(false);

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let full_path = if path.starts_with('/') {
            PathBuf::from(path)
        } else {
            base_dir.join(path)
        };

        let mut content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| Error::ToolExecution(format!("Failed to read file: {}", e)))?;

        if replace_all {
            content = content.replace(old_string, new_string);
        } else {
            if !content.contains(old_string) {
                return Ok(ToolResult::error(
                    "edit_file",
                    format!("Old string not found in file: {}", old_string.chars().take(50).collect::<String>()),
                ));
            }
            content = content.replacen(old_string, new_string, 1);
        }

        tokio::fs::write(&full_path, content)
            .await
            .map_err(|e| Error::ToolExecution(format!("Failed to write file: {}", e)))?;

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "edit_file".to_string(),
            output: format!("Successfully edited {}", full_path.display()),
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "path": full_path.display().to_string(),
                "replace_all": replace_all,
            })),
        })
    }
}

/// Glob pattern matching tool
pub struct GlobTool;

impl GlobTool {
    pub fn definition() -> Tool {
        Tool::new("glob", "Glob", "Find files matching a glob pattern")
            .with_parameter(ToolParameterSchema::new("pattern", "string", "Glob pattern (e.g., '*.rs', 'src/**/*.ts')"))
            .with_parameter(ToolParameterSchema::new("path", "string", "Directory to search in").optional())
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for GlobTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'pattern' parameter".to_string()))?;

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let search_path = params["path"].as_str().map(|p| {
            if p.starts_with('/') { PathBuf::from(p) } else { base_dir.join(p) }
        }).unwrap_or_else(|| base_dir.clone());

        let matches = glob::glob(&search_path.join(pattern).to_string_lossy())
            .map_err(|e| Error::ToolExecution(format!("Invalid glob pattern: {}", e)))?;

        let mut results = Vec::new();
        for entry in matches.flatten() {
            match entry.strip_prefix(&base_dir) {
                Ok(relative_path) => {
                    results.push(relative_path.display().to_string());
                }
                Err(_) => {
                    // Entry is outside base_dir, use full path
                    results.push(entry.display().to_string());
                }
            }
        }

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "glob".to_string(),
            output: if results.is_empty() {
                "No files found matching pattern".to_string()
            } else {
                results.join("\n")
            },
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "pattern": pattern,
                "match_count": results.len(),
            })),
        })
    }
}

/// Grep search tool
pub struct GrepTool;

impl GrepTool {
    pub fn definition() -> Tool {
        Tool::new("grep", "Grep", "Search for text in files")
            .with_parameter(ToolParameterSchema::new("pattern", "string", "Regex pattern to search for"))
            .with_parameter(ToolParameterSchema::new("path", "string", "File or directory to search in").optional())
            .with_parameter(ToolParameterSchema::new("include", "string", "File pattern to include (e.g., '*.rs')").optional())
            .requires_permission(false)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for GrepTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'pattern' parameter".to_string()))?;

        let include = params["include"].as_str();

        let start = Instant::now();

        let base_dir = ctx.current_directory.as_ref().map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let search_path = params["path"].as_str()
            .map(|p| if p.starts_with('/') { PathBuf::from(p) } else { base_dir.join(p) })
            .unwrap_or_else(|| base_dir.clone());

        let regex = regex::Regex::new(pattern)
            .map_err(|e| Error::ToolExecution(format!("Invalid regex pattern: {}", e)))?;

        let mut results = Vec::new();

        let walk = ignore::Walk::new(&search_path);

        for entry in walk {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() {
                    if let Some(include) = include {
                        let pattern = include.trim_start_matches('*');
                        if !path.to_string_lossy().ends_with(pattern) {
                            continue;
                        }
                    }

                    if let Ok(content) = tokio::fs::read_to_string(path).await {
                        for (line_num, line) in content.lines().enumerate() {
                            if regex.is_match(line) {
                                let relative_path = path.strip_prefix(&base_dir).unwrap_or(path);
                                results.push(format!("{}:{}:{}", relative_path.display(), line_num + 1, line));
                            }
                        }
                    }
                }
            }
        }

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "grep".to_string(),
            output: if results.is_empty() {
                "No matches found".to_string()
            } else {
                results.join("\n")
            },
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "pattern": pattern,
                "match_count": results.len(),
            })),
        })
    }
}

// ==================== Git Tools ====================

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

/// Web fetch tool - fetch content from a URL
pub struct WebFetchTool;

impl WebFetchTool {
    pub fn definition() -> Tool {
        Tool::new("web_fetch", "Web Fetch", "Fetch and return content from a URL")
            .with_parameter(ToolParameterSchema::new("url", "string", "The URL to fetch"))
            .with_parameter(ToolParameterSchema::new("method", "string", "HTTP method (GET, POST, etc.)").optional())
            .with_parameter(ToolParameterSchema::new("headers", "object", "HTTP headers").optional())
            .with_parameter(ToolParameterSchema::new("body", "string", "Request body for POST requests").optional())
            .with_parameter(ToolParameterSchema::new("timeout", "number", "Request timeout in seconds").optional())
            .requires_permission(true)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for WebFetchTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'url' parameter".to_string()))?;

        let method = params["method"].as_str().unwrap_or("GET");
        let timeout_secs = params["timeout"].as_f64().unwrap_or(30.0);
        let _headers = params["headers"].as_object();
        let _body = params["body"].as_str();

        let start = Instant::now();

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs_f64(timeout_secs))
            .build()
            .map_err(|e| Error::ToolExecution(format!("Failed to create HTTP client: {}", e)))?;

        let request = match method.to_uppercase().as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "HEAD" => client.head(url),
            _ => client.get(url),
        };

        // Add headers if provided
        let request = if let Some(headers) = _headers {
            let mut req = request;
            for (key, value) in headers {
                if let Some(value_str) = value.as_str() {
                    req = req.header(key, value_str);
                }
            }
            req
        } else {
            request
        };

        // Add body if provided
        let request = if let Some(body) = _body {
            request.body(body.to_string())
        } else {
            request
        };

        let response = request
            .send()
            .await
            .map_err(|e| Error::ToolExecution(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        let headers = response.headers().clone();

        let body = response
            .text()
            .await
            .map_err(|e| Error::ToolExecution(format!("Failed to read response body: {}", e)))?;

        let duration = start.elapsed().as_millis() as u64;

        // Format response
        let mut output = format!("Status: {}\n\n", status);
        output.push_str("Headers:\n");
        for (name, value) in headers.iter() {
            if let Ok(value_str) = value.to_str() {
                output.push_str(&format!("  {}: {}\n", name, value_str));
            }
        }
        output.push_str(&format!("\nBody ({} bytes):\n", body.len()));
        output.push_str(&body);

        Ok(ToolResult {
            tool_id: "web_fetch".to_string(),
            output,
            is_error: !status.is_success(),
            execution_time_ms: duration,
            metadata: Some(json!({
                "url": url,
                "status": status.as_u16(),
                "content_type": headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("unknown"),
                "content_length": body.len(),
            })),
        })
    }
}

/// Web search tool - search the web
pub struct WebSearchTool;

impl WebSearchTool {
    pub fn definition() -> Tool {
        Tool::new("web_search", "Web Search", "Search the web for information")
            .with_parameter(ToolParameterSchema::new("query", "string", "The search query"))
            .with_parameter(ToolParameterSchema::new("num_results", "number", "Number of results (default: 10)").optional())
            .requires_permission(true)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for WebSearchTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'query' parameter".to_string()))?;

        let start = Instant::now();

        // Use DuckDuckGo for search (no API key needed)
        let search_url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencoding::encode(query)
        );

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| Error::ToolExecution(format!("Failed to create HTTP client: {}", e)))?;

        let response = client
            .get(&search_url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
            .map_err(|e| Error::ToolExecution(format!("Search request failed: {}", e)))?;

        let html = response
            .text()
            .await
            .map_err(|e| Error::ToolExecution(format!("Failed to read response: {}", e)))?;

        // Parse results using regex
        let mut output = format!("Search results for '{}':\n\n", query);

        // Extract results from HTML
        let re = regex::Regex::new(r#"<a[^>]*class="result__a"[^>]*href="([^"]*)"[^>]*>([^<]*)</a>"#)
            .map_err(|_| Error::ToolExecution("Failed to compile regex".to_string()))?;

        let mut count = 0;
        for caps in re.captures_iter(&html).take(10) {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let title = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            // Clean up DuckDuckGo redirect URLs
            let clean_url: String = if url.starts_with("/l/?uddg=") {
                url.split("uddg=")
                    .nth(1)
                    .and_then(|s| s.split('&').next())
                    .and_then(|s| urlencoding::decode(s).ok())
                    .map(|cow| cow.to_string())
                    .unwrap_or_else(|| url.to_string())
            } else {
                url.to_string()
            };

            count += 1;
            output.push_str(&format!("{}. {}\n", count, title));
            output.push_str(&format!("   {}\n\n", clean_url));
        }

        if count == 0 {
            output.push_str("No results found.");
        }

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "web_search".to_string(),
            output,
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "query": query,
                "result_count": count,
            })),
        })
    }
}
