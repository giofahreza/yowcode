use crate::message::{Message, MessageRole, MessageContent};
use crate::types::FilePath;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Builder for collecting context from a project
pub struct ContextBuilder {
    base_path: PathBuf,
    ignore_patterns: Vec<String>,
    max_files: usize,
    max_tokens: usize,
}

impl ContextBuilder {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            ignore_patterns: vec![
                "node_modules".to_string(),
                ".git".to_string(),
                "target".to_string(),
                "vendor".to_string(),
                ".venv".to_string(),
                "__pycache__".to_string(),
                "*.pyc".to_string(),
                ".DS_Store".to_string(),
            ],
            max_files: 100,
            max_tokens: 50000,
        }
    }

    pub fn with_ignore_patterns(mut self, patterns: Vec<String>) -> Self {
        self.ignore_patterns.extend(patterns);
        self
    }

    pub fn with_max_files(mut self, max: usize) -> Self {
        self.max_files = max;
        self
    }

    pub fn with_max_tokens(mut self, max: usize) -> Self {
        self.max_tokens = max;
        self
    }

    /// Collect files from the project
    pub async fn collect_files(&self) -> Result<Vec<FilePath>> {
        let mut files = Vec::new();
        let ignore_patterns = self.ignore_patterns.clone();
        let max_files = self.max_files;

        let walker = WalkBuilder::new(&self.base_path)
            .hidden(true)
            .git_ignore(true)
            .filter_entry(move |entry| {
                let name = entry.file_name().to_string_lossy();
                !ignore_patterns.iter().any(|p| name.contains(p))
            })
            .build();

        for entry in walker {
            if let Ok(entry) = entry {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    files.push(entry.path().to_path_buf());
                    if files.len() >= max_files {
                        break;
                    }
                }
            }
        }

        Ok(files)
    }

    /// Read file contents
    pub async fn read_files(&self, files: &[FilePath]) -> Result<HashMap<FilePath, String>> {
        let mut contents = HashMap::new();

        for file in files {
            if let Ok(content) = tokio::fs::read_to_string(file).await {
                contents.insert(file.clone(), content);
            }
        }

        Ok(contents)
    }

    /// Build context messages
    pub async fn build_context_messages(
        &self,
        query: &str,
    ) -> Result<Vec<Message>> {
        let files = self.collect_files().await?;
        let file_contents = self.read_files(&files).await?;

        let mut messages = Vec::new();

        // System message with project structure
        let project_structure = files
            .iter()
            .map(|p| {
                p.strip_prefix(&self.base_path)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");

        messages.push(Message::system(format!(
            "You are working in a project at: {}\n\nProject structure:\n{}",
            self.base_path.display(),
            project_structure
        )));

        // Add file contents for relevant files (simplified - in real impl would do semantic search)
        let mut context_tokens = 0;
        for (file, content) in file_contents {
            if context_tokens + content.len() > self.max_tokens {
                break;
            }
            context_tokens += content.len();

            let relative_path = file
                .strip_prefix(&self.base_path)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();

            messages.push(Message::system(format!(
                "File: {}\n```\n{}\n```",
                relative_path, content
            )));
        }

        messages.push(Message::user_text(query));

        Ok(messages)
    }
}

/// Token estimator
pub struct TokenEstimator;

impl TokenEstimator {
    /// Estimate tokens for text (rough approximation: ~4 chars per token)
    pub fn estimate(text: &str) -> usize {
        text.len() / 4
    }

    /// Estimate tokens for messages
    pub fn estimate_messages(messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|m| {
                match &m.content {
                    MessageContent::Text(t) => Self::estimate(t),
                    MessageContent::TextWithImages { text, .. } => Self::estimate(text),
                    MessageContent::ToolCalls(calls) => {
                        calls.iter().map(|c| Self::estimate(&c.name) + Self::estimate(&c.arguments.to_string())).sum()
                    }
                    MessageContent::ToolResult { content, .. } => Self::estimate(content),
                    MessageContent::Thinking(t) => Self::estimate(t),
                }
            })
            .sum()
    }
}

/// Context compression for managing token limits
pub struct ContextCompressor {
    max_tokens: usize,
}

impl ContextCompressor {
    pub fn new(max_tokens: usize) -> Self {
        Self { max_tokens }
    }

    /// Compress a list of messages to fit within token limits
    pub fn compress(&self, messages: &mut Vec<Message>) {
        let current_tokens = TokenEstimator::estimate_messages(messages);

        if current_tokens <= self.max_tokens {
            return;
        }

        // Keep system messages and recent messages
        let system_messages: Vec<_> = messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .cloned()
            .collect();

        let mut user_assistant_messages: Vec<_> = messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .cloned()
            .collect();

        // Remove oldest messages until we fit
        while TokenEstimator::estimate_messages(&user_assistant_messages) + TokenEstimator::estimate_messages(&system_messages) > self.max_tokens {
            if user_assistant_messages.len() <= 2 {
                break;
            }
            user_assistant_messages.remove(0);
        }

        messages.clear();
        messages.extend(system_messages);
        messages.extend(user_assistant_messages);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimator() {
        let text = "Hello, world!";
        let tokens = TokenEstimator::estimate(text);
        assert!(tokens > 0);
    }

    #[test]
    fn test_context_compressor() {
        let compressor = ContextCompressor::new(100);

        let mut messages = vec![
            Message::system("System message"),
            Message::user_text("a".repeat(1000)),
            Message::text("b".repeat(1000)),
            Message::user_text("c".repeat(1000)),
        ];

        let before = messages.len();
        compressor.compress(&mut messages);
        let after = messages.len();

        assert!(after < before);
    }
}
