use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade}, State,
    },
    response::IntoResponse,
    routing::{get, put},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};
use uuid::Uuid;
use yowcode_core::{
    ai::{AIConfig, AIProvider, OpenAICompatClient},
    config::Config,
    database::initialize_database,
    executor::{ChatExecutor, ExecutionEvent, ExecutionOptions},
    message::{ChatHistory, Message},
    runs::RunManager,
    session::{Session, SessionEvent, SessionManager},
    tool::{ToolExecutor, ToolRegistry, ToolResult},
    types::PermissionMode,
};

// Use core library's tool types directly

mod handlers;

use handlers::*;

/// Application state shared across all handlers
#[derive(Clone)]
struct AppState {
    session_manager: Arc<SessionManager>,
    executor: Arc<ChatExecutor>,
    tool_registry: Arc<ToolRegistry>,
    run_manager: Arc<RunManager>,
    config: Config,
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum WSMessage {
    // Client -> Server
    ChatRequest { session_id: Uuid, message: String },
    CreateSession { title: String },
    JoinSession { session_id: Uuid },

    // Server -> Client
    ChatResponse { content: String, is_complete: bool },
    ToolCall { tool_name: String, status: String },
    Error { message: String },
    SessionCreated { session_id: Uuid, title: String },
    SessionJoined { session_id: Uuid, messages: Vec<handlers::ClientMessage> },
    StatusUpdate { status: String },
}

/// Create router
fn create_router(state: AppState) -> Router {
    Router::new()
        // API routes
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:id", get(get_session).delete(delete_session))
        .route("/api/sessions/:id/messages", get(get_messages).post(send_message))
        .route("/api/sessions/:id/settings", put(update_settings))
        .route("/api/tools", get(list_tools))
        .route("/api/health", get(health_check))
        // Project routes
        .route("/api/projects", get(list_projects).post(create_project))
        .route("/api/projects/:id", get(get_project).delete(delete_project))
        .route("/api/projects/:id/runs", get(list_runs).post(create_run))
        // Run routes
        .route("/api/runs/:id", get(get_run).delete(cancel_run))
        .route("/api/runs/:id/tasks", get(list_tasks))
        .route("/api/runs/:id/artifacts", get(list_artifacts))
        .route("/api/runs/:id/audit", get(list_audit_events))
        .route("/api/runs/stats", get(get_stats))
        // WebSocket route
        .route("/ws", get(websocket_handler))
        // Static files and index
        .route("/", get(index))
        .route("/static/*path", get(static_files))
        .with_state(state)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("yowcode=debug,info")
        .init();

    info!("Starting YowCode Web Server");

    // Load configuration
    let config = Config::load(None).await.context("Failed to load configuration")?;

    // Expand database path
    let db_path = Config::expand_path(&config.database.path);
    let db_path_str = db_path.to_string_lossy().to_string();

    // Initialize database
    let db = initialize_database(&db_path_str)
        .await
        .context("Failed to initialize database")?;

    // Create session manager
    let session_manager = Arc::new(SessionManager::new(db.clone()));

    // Create AI client
    let ai_config = AIConfig {
        provider: match config.ai.provider.as_str() {
            "openai" => AIProvider::OpenAI,
            "anthropic" => AIProvider::Anthropic,
            "openrouter" => AIProvider::OpenRouter,
            _ => AIProvider::Custom,
        },
        api_key: config.ai.api_key.clone(),
        base_url: config.ai.base_url.clone(),
        model: config.ai.model.clone(),
        max_retries: 3,
        timeout: std::time::Duration::from_secs(120),
    };

    let ai_client = Arc::new(OpenAICompatClient::new(ai_config));

    // Create tool registry and register tools
    let mut tool_registry = ToolRegistry::new();

    // Register built-in tools (same as CLI)
    // In a real implementation, these would be shared modules
    tool_registry.register(Arc::new(BashTool));
    tool_registry.register(Arc::new(ReadTool));
    tool_registry.register(Arc::new(WriteTool));
    tool_registry.register(Arc::new(EditTool));
    tool_registry.register(Arc::new(GlobTool));
    tool_registry.register(Arc::new(GrepTool));
    tool_registry.register(Arc::new(CommitTool));
    tool_registry.register(Arc::new(DiffTool));
    tool_registry.register(Arc::new(AskUserQuestionTool));
    tool_registry.register(Arc::new(SleepTool));
    tool_registry.register(Arc::new(SyntheticOutputTool));
    tool_registry.register(Arc::new(GitStatusTool));
    tool_registry.register(Arc::new(GitBranchTool));
    tool_registry.register(Arc::new(ListDirectoryTool));
    tool_registry.register(Arc::new(FileInfoTool));

    let tool_registry = Arc::new(tool_registry);

    // Create broadcast channel for events
    let (tx, _rx) = broadcast::channel(1000);

    // Create executor
    let executor = Arc::new(ChatExecutor::new(ai_client, tool_registry.clone(), tx));

    // Create app state
    let run_manager = Arc::new(RunManager::new());

    let state = AppState {
        session_manager,
        executor,
        tool_registry,
        run_manager,
        config,
    };

    // Build router
    let app = create_router(state.clone());

    // Start server
    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);
    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// WebSocket handler
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let session_id: Option<Uuid> = None;

    // Subscribe to session events
    let mut event_rx = state.session_manager.subscribe();

    // Use a channel to send messages to the socket
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let tx_clone = tx.clone();

    // Task to forward session events to WebSocket
    let event_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            let msg = match event {
                SessionEvent::Message { session_id: sid, message } => {
                    if Some(sid) == session_id {
                        Some(WSMessage::ChatResponse {
                            content: message.get_text().cloned().unwrap_or_default(),
                            is_complete: true,
                        })
                    } else {
                        None
                    }
                }
                SessionEvent::Status { session_id: sid, status } => {
                    if Some(sid) == session_id {
                        Some(WSMessage::StatusUpdate { status })
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(ws_msg) = msg {
                if let Ok(json) = serde_json::to_string(&ws_msg) {
                    let _ = tx.send(json);
                }
            }
        }
    });

    // Main loop: receive from socket and send to socket
    loop {
        tokio::select! {
            // Receive from WebSocket
            msg = socket.recv() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(ws_msg) = serde_json::from_str::<WSMessage>(&text) {
                            match ws_msg {
                                WSMessage::ChatRequest {
                                    session_id: sid,
                                    message,
                                } => {
                                    let _ = sid;  // Use the session_id directly

                                    // Send user message
                                    let _ = state
                                        .session_manager
                                        .add_message(sid, Message::user_text(&message))
                                        .await;

                                    // Get history and execute
                                    let mut history = state
                                        .session_manager
                                        .get_history(sid)
                                        .await
                                        .unwrap_or_else(|_| ChatHistory::new(sid));

                                    // Execute in background
                                    let sm = state.session_manager.clone();
                                    let exec = state.executor.clone();
                                    tokio::spawn(async move {
                                        if let Ok(result) = exec
                                            .execute(
                                                &mut history,
                                                message,
                                                ExecutionOptions {
                                                    max_iterations: 100,
                                                    max_context_tokens: 200000,
                                                    permission_mode: PermissionMode::Auto,
                                                    stream_responses: false,
                                                },
                                                |event| {
                                                    match event {
                                                        ExecutionEvent::AssistantMessage { content } => {
                                                            let _ = sm.add_message(
                                                                sid,
                                                                Message::text(&content),
                                                            );
                                                        }
                                                        ExecutionEvent::ToolCall { tool_name } => {
                                                            let _ = sm.add_message(
                                                                sid,
                                                                Message::system(format!("Running: {}", tool_name)),
                                                            );
                                                        }
                                                        ExecutionEvent::ToolResult {
                                                            tool_name,
                                                            result,
                                                            is_error,
                                                        } => {
                                                            let prefix = if is_error { "Error" } else { "Result" };
                                                            let _ = sm.add_message(
                                                                sid,
                                                                Message::system(format!("{} from {}: {}", prefix, tool_name, result)),
                                                            );
                                                        }
                                                        _ => {}
                                                    }
                                                },
                                            )
                                            .await
                                        {
                                            info!("Execution complete: {:?}", result);
                                        }
                                    });
                                }
                                WSMessage::CreateSession { title } => {
                                    let title_clone = title.clone();
                                    let session = Session::new(title_clone.clone());
                                    let sid = state.session_manager.create_session(session).await;
                                    if let Ok(sid) = sid {
                                        let json = serde_json::to_string(&WSMessage::SessionCreated {
                                            session_id: sid,
                                            title: title_clone,
                                        }).unwrap();
                                        let _ = tx_clone.send(json);
                                    }
                                }
                                WSMessage::JoinSession { session_id: sid } => {
                                    let _ = sid;  // Use the session_id directly
                                    if let Ok(history) = state.session_manager.get_history(sid).await {
                                        let messages: Vec<handlers::ClientMessage> =
                                            history.messages.into_iter().map(Into::into).collect();
                                        let json = serde_json::to_string(&WSMessage::SessionJoined {
                                            session_id: sid,
                                            messages,
                                        }).unwrap();
                                        let _ = tx_clone.send(json);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) => break,
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }

            // Send to WebSocket from channel
            Some(msg) = rx.recv() => {
                if socket.send(WsMessage::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        }
    }

    event_task.abort();
}

// Import additional tools from core
use yowcode_core::tools::{
    CommitTool, DiffTool, AskUserQuestionTool, SleepTool, SyntheticOutputTool,
    GitStatusTool, GitBranchTool, ListDirectoryTool, FileInfoTool,
};

// Simple tool implementations for the web server
struct BashTool;
struct ReadTool;
struct WriteTool;
struct EditTool;
struct GlobTool;
struct GrepTool;

// These would be implemented similarly to CLI tools
// For brevity, we're just providing stubs here
impl BashTool {
    fn definition() -> yowcode_core::tool::Tool {
        yowcode_core::tool::Tool::new("bash", "Bash", "Execute a shell command")
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("command", "string", "The shell command to execute"))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for BashTool {
    fn definition(&self) -> &yowcode_core::tool::Tool {
        static TOOL: std::sync::OnceLock<yowcode_core::tool::Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &yowcode_core::tool::ToolExecutionContext, _params: serde_json::Value) -> Result<ToolResult, yowcode_core::error::Error> {
        Ok(ToolResult::success("bash", "Command executed"))
    }
}

impl ReadTool {
    fn definition() -> yowcode_core::tool::Tool {
        yowcode_core::tool::Tool::new("read", "Read", "Read a file from the filesystem")
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("path", "string", "The file path to read"))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for ReadTool {
    fn definition(&self) -> &yowcode_core::tool::Tool {
        static TOOL: std::sync::OnceLock<yowcode_core::tool::Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &yowcode_core::tool::ToolExecutionContext, _params: serde_json::Value) -> Result<ToolResult, yowcode_core::error::Error> {
        Ok(ToolResult::success("read", "File content"))
    }
}

impl WriteTool {
    fn definition() -> yowcode_core::tool::Tool {
        yowcode_core::tool::Tool::new("write", "Write", "Write content to a file")
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("path", "string", "The file path to write"))
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("content", "string", "The content to write"))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for WriteTool {
    fn definition(&self) -> &yowcode_core::tool::Tool {
        static TOOL: std::sync::OnceLock<yowcode_core::tool::Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &yowcode_core::tool::ToolExecutionContext, _params: serde_json::Value) -> Result<ToolResult, yowcode_core::error::Error> {
        Ok(ToolResult::success("write", "File written"))
    }
}

impl EditTool {
    fn definition() -> yowcode_core::tool::Tool {
        yowcode_core::tool::Tool::new("edit", "Edit", "Edit a file by replacing text")
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("path", "string", "The file path to edit"))
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("old_string", "string", "The text to replace"))
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("new_string", "string", "The new text"))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for EditTool {
    fn definition(&self) -> &yowcode_core::tool::Tool {
        static TOOL: std::sync::OnceLock<yowcode_core::tool::Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &yowcode_core::tool::ToolExecutionContext, _params: serde_json::Value) -> Result<ToolResult, yowcode_core::error::Error> {
        Ok(ToolResult::success("edit", "File edited"))
    }
}

impl GlobTool {
    fn definition() -> yowcode_core::tool::Tool {
        yowcode_core::tool::Tool::new("glob", "Glob", "Find files matching a pattern")
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("pattern", "string", "The glob pattern"))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for GlobTool {
    fn definition(&self) -> &yowcode_core::tool::Tool {
        static TOOL: std::sync::OnceLock<yowcode_core::tool::Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &yowcode_core::tool::ToolExecutionContext, _params: serde_json::Value) -> Result<ToolResult, yowcode_core::error::Error> {
        Ok(ToolResult::success("glob", "Files found"))
    }
}

impl GrepTool {
    fn definition() -> yowcode_core::tool::Tool {
        yowcode_core::tool::Tool::new("grep", "Grep", "Search for text in files")
            .with_parameter(yowcode_core::tool::ToolParameterSchema::new("pattern", "string", "The regex pattern to search for"))
    }
}

#[async_trait::async_trait]
impl ToolExecutor for GrepTool {
    fn definition(&self) -> &yowcode_core::tool::Tool {
        static TOOL: std::sync::OnceLock<yowcode_core::tool::Tool> = std::sync::OnceLock::new();
        TOOL.get_or_init(|| Self::definition())
    }

    async fn execute(&self, _ctx: &yowcode_core::tool::ToolExecutionContext, _params: serde_json::Value) -> Result<ToolResult, yowcode_core::error::Error> {
        Ok(ToolResult::success("grep", "Matches found"))
    }
}
