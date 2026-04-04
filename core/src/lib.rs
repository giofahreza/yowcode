// yowcode-core: Shared core library for yowcode
//
// This library contains all shared business logic, types, and traits
// used by both the CLI and web interfaces.

pub mod ai;
pub mod commands;
pub mod config;
pub mod context;
pub mod database;
pub mod error;
pub mod executor;
pub mod message;
pub mod runs;
pub mod session;
pub mod tool;
pub mod tools;
pub mod types;

// Re-exports for convenience
pub use error::{Error, Result};
pub use message::{Message, MessageContent, MessageRole, ToolCall};
pub use session::{Session, SessionManager, SessionSettings};
pub use tool::{Tool, ToolExecutionContext, ToolExecutor, ToolPermission, ToolRegistry, ToolResult};
pub use types::*;
pub use ai::{AIProvider, AIStreamEvent, ChatCompletionResponse, AIClient, AIConfig, OpenAICompatClient};
pub use runs::{Run, RunConfig, RunStatus, Task, TaskStatus, Artifact, AuditEvent, RunManager, RunExecutor, RunStats, RunHandle};
