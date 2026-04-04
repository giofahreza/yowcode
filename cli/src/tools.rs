use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;
use yowcode_core::{
    tool::{Tool, ToolExecutionContext, ToolExecutor, ToolParameterSchema, ToolResult},
    Error,
};

/// Bash command execution tool
pub struct BashTool;

impl BashTool {
    fn definition() -> Tool {
        Tool::new("bash", "Bash", "Execute a shell command")
            .with_parameter(ToolParameterSchema::new("command", "string", "The shell command to execute"))
            .requires_permission(true)
            .is_destructive(false)
    }
}

#[async_trait]
impl ToolExecutor for BashTool {
    fn definition(&self) -> &Tool {
        static TOOL: std::sync::OnceLock<Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult, Error> {
        let command = params["command"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'command' parameter".to_string()))?;

        let start = Instant::now();

        // Execute the command
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(dir) = &ctx.current_directory {
            cmd.current_dir(dir);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| Error::CommandExecution(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let duration = start.elapsed().as_millis() as u64;

        let result = if output.status.success() {
            if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n{}", stdout, stderr)
            }
        } else {
            return Ok(ToolResult {
                tool_id: "bash".to_string(),
                output: if stdout.is_empty() {
                    stderr
                } else {
                    format!("{}\n{}", stdout, stderr)
                },
                is_error: true,
                execution_time_ms: duration,
                metadata: Some(json!({
                    "exit_code": output.status.code(),
                })),
            });
        };

        Ok(ToolResult {
            tool_id: "bash".to_string(),
            output: result,
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "exit_code": output.status.code(),
            })),
        })
    }
}

/// File read tool
pub struct ReadTool;

impl ReadTool {
    fn definition() -> Tool {
        Tool::new("read", "Read", "Read a file from the filesystem")
            .with_parameter(ToolParameterSchema::new("path", "string", "The file path to read"))
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

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult, Error> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'path' parameter".to_string()))?;

        let full_path = if let Some(base) = &ctx.current_directory {
            PathBuf::from(base).join(path)
        } else {
            PathBuf::from(path)
        };

        let start = Instant::now();

        let content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| Error::IO(e))?;

        let content_len = content.len();
        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "read".to_string(),
            output: content,
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "path": full_path,
                "size": content_len,
            })),
        })
    }
}

/// File write tool
pub struct WriteTool;

impl WriteTool {
    fn definition() -> Tool {
        Tool::new("write", "Write", "Write content to a file (creates or overwrites)")
            .with_parameter(ToolParameterSchema::new("path", "string", "The file path to write"))
            .with_parameter(ToolParameterSchema::new("content", "string", "The content to write"))
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

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult, Error> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'path' parameter".to_string()))?;

        let content = params["content"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'content' parameter".to_string()))?;

        let full_path = if let Some(base) = &ctx.current_directory {
            PathBuf::from(base).join(path)
        } else {
            PathBuf::from(path)
        };

        let start = Instant::now();

        // Create parent directory if it doesn't exist
        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::IO(e))?;
        }

        tokio::fs::write(&full_path, content)
            .await
            .map_err(|e| Error::IO(e))?;

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "write".to_string(),
            output: format!("Wrote {} bytes to {}", content.len(), full_path.display()),
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "path": full_path,
                "size": content.len(),
            })),
        })
    }
}

/// File edit tool
pub struct EditTool;

impl EditTool {
    fn definition() -> Tool {
        Tool::new("edit", "Edit", "Edit a file by replacing text")
            .with_parameter(ToolParameterSchema::new("path", "string", "The file path to edit"))
            .with_parameter(ToolParameterSchema::new("old_string", "string", "The text to replace"))
            .with_parameter(ToolParameterSchema::new("new_string", "string", "The new text"))
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

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult, Error> {
        let path = params["path"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'path' parameter".to_string()))?;

        let old_string = params["old_string"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'old_string' parameter".to_string()))?;

        let new_string = params["new_string"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'new_string' parameter".to_string()))?;

        let full_path = if let Some(base) = &ctx.current_directory {
            PathBuf::from(base).join(path)
        } else {
            PathBuf::from(path)
        };

        let start = Instant::now();

        let content = tokio::fs::read_to_string(&full_path)
            .await
            .map_err(|e| Error::IO(e))?;

        if !content.contains(old_string) {
            return Ok(ToolResult {
                tool_id: "edit".to_string(),
                output: format!("Could not find '{}' in {}", old_string, full_path.display()),
                is_error: true,
                execution_time_ms: 0,
                metadata: None,
            });
        }

        let new_content = content.replace(old_string, new_string);

        tokio::fs::write(&full_path, new_content)
            .await
            .map_err(|e| Error::IO(e))?;

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "edit".to_string(),
            output: format!("Replaced '{}' in {}", old_string, full_path.display()),
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "path": full_path,
                "replacements": content.matches(old_string).count(),
            })),
        })
    }
}

/// Glob tool for file pattern matching
pub struct GlobTool;

impl GlobTool {
    fn definition() -> Tool {
        Tool::new("glob", "Glob", "Find files matching a pattern")
            .with_parameter(ToolParameterSchema::new("pattern", "string", "The glob pattern (e.g., **/*.rs)"))
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

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult, Error> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'pattern' parameter".to_string()))?;

        let start = Instant::now();

        let base_dir = ctx
            .current_directory
            .as_ref()
            .map(|d| PathBuf::from(d))
            .unwrap_or_else(|| std::env::current_dir().unwrap());

        let mut matches = Vec::new();

        // Simple glob implementation (for production, use glob crate)
        for entry in walkdir::WalkDir::new(&base_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                let relative_path = path
                    .strip_prefix(&base_dir)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();

                // Simple pattern matching
                let glob_pattern = pattern.replace("**", "*").replace("*", ".*");
                if let Ok(re) = regex::Regex::new(&format!("^{}$", glob_pattern)) {
                    if re.is_match(&relative_path) {
                        matches.push(relative_path);
                    }
                }
            }
        }

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "glob".to_string(),
            output: if matches.is_empty() {
                "No files found".to_string()
            } else {
                matches.join("\n")
            },
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "count": matches.len(),
            })),
        })
    }
}

/// Grep tool for content search
pub struct GrepTool;

impl GrepTool {
    fn definition() -> Tool {
        Tool::new("grep", "Grep", "Search for text in files")
            .with_parameter(ToolParameterSchema::new("pattern", "string", "The regex pattern to search for"))
            .with_parameter(ToolParameterSchema::new("path", "string", "The directory to search (optional)").optional())
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

    async fn execute(&self, ctx: &ToolExecutionContext, params: serde_json::Value) -> Result<ToolResult, Error> {
        let pattern = params["pattern"]
            .as_str()
            .ok_or_else(|| Error::ToolExecution("Missing 'pattern' parameter".to_string()))?;

        let search_path = params["path"]
            .as_str()
            .or(ctx.current_directory.as_deref())
            .unwrap_or(".");

        let start = Instant::now();

        let mut matches = Vec::new();

        for entry in walkdir::WalkDir::new(search_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    for (line_num, line) in content.lines().enumerate() {
                        if line.contains(pattern) {
                            matches.push(format!("{}:{}:{}", path.display(), line_num + 1, line));
                        }
                    }
                }
            }
        }

        let duration = start.elapsed().as_millis() as u64;

        Ok(ToolResult {
            tool_id: "grep".to_string(),
            output: if matches.is_empty() {
                "No matches found".to_string()
            } else {
                matches.join("\n")
            },
            is_error: false,
            execution_time_ms: duration,
            metadata: Some(json!({
                "count": matches.len(),
            })),
        })
    }
}
