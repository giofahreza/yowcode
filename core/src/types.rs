use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

impl fmt::Display for InterfaceMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InterfaceMode::CLI => write!(f, "CLI"),
            InterfaceMode::Web => write!(f, "Web"),
            InterfaceMode::Both => write!(f, "Both"),
        }
    }
}

/// Unique identifier for a tool
pub type ToolId = String;

/// Unique identifier for a command
pub type CommandId = String;

/// Unique identifier for a skill
pub type SkillId = String;

/// File path representation
pub type FilePath = std::path::PathBuf;

/// Git diff representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GitDiff {
    pub file_path: String,
    pub old_content: String,
    pub new_content: String,
}

/// Execution mode for operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Execute on the host machine
    Host,
    /// Execute in a container (e.g., Docker)
    Container,
    /// Execute in a git worktree
    Worktree,
    /// Execute in a copy of the workspace
    Copy,
}

/// Permission mode for tool execution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionMode {
    /// Ask for permission before every action
    AlwaysAsk,
    /// Automatically approve non-destructive actions
    Default,
    /// Automatically approve everything
    Auto,
    /// Ask during plan mode, auto during execution
    Plan,
    /// Bypass all permission checks
    Bypass,
}

/// Project configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub execution_mode: ExecutionMode,
    pub container_image: Option<String>,
    pub env_vars: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Project {
    pub fn new(path: impl Into<String>, name: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            path: path.into(),
            description: None,
            execution_mode: ExecutionMode::Host,
            container_image: None,
            env_vars: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_execution_mode(mut self, mode: ExecutionMode) -> Self {
        self.execution_mode = mode;
        self
    }

    pub fn with_container_image(mut self, image: impl Into<String>) -> Self {
        self.container_image = Some(image.into());
        self
    }
}

/// Run configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub id: Uuid,
    pub project_id: Uuid,
    pub prompt: String,
    pub is_continuous: bool,
    pub max_cycles: Option<u32>,
    pub status: RunStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Run status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Task in a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub run_id: Uuid,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub owner: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Task status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Blocked,
}

/// Artifact generated during a run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub run_id: Uuid,
    pub task_id: Option<Uuid>,
    pub artifact_type: ArtifactType,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Artifact type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactType {
    File,
    Diff,
    Report,
    Log,
    Image,
    Other(String),
}

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub run_id: Option<Uuid>,
    pub event_type: String,
    pub data: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// UI theme
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    Auto,
}

/// Interface mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InterfaceMode {
    CLI,
    Web,
    Both,
}

/// File edit operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EditOperation {
    Replace { old_string: String, new_string: String },
    Insert { position: usize, content: String },
    Delete { range: std::ops::Range<usize> },
}
