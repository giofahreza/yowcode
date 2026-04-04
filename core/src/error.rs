use thiserror::Error;

/// Core error type for yowcode
#[derive(Error, Debug)]
pub enum Error {
    #[error("AI provider error: {0}")]
    AI(#[from] AIError),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Tool execution error: {0}")]
    ToolExecution(String),

    #[error("Tool permission denied: {0}")]
    ToolPermissionDenied(String),

    #[error("Session not found: {0}")]
    SessionNotFound(uuid::Uuid),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),

    #[error("Command execution failed: {0}")]
    CommandExecution(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Run error: {0}")]
    RunError(String),

    #[error("Other: {0}")]
    Other(String),
}

/// AI-specific errors
#[derive(Error, Debug)]
pub enum AIError {
    #[error("API error: {0}")]
    Api(String),

    #[error("Rate limit exceeded")]
    RateLimit,

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Token limit exceeded")]
    TokenLimit,
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, Error>;
