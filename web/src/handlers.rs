use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;
use yowcode_core::{
    message::Message,
    runs::RunConfig,
    session::{Session, SessionSettings},
};

use super::AppState;

/// Message format for client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Message> for ClientMessage {
    fn from(msg: Message) -> Self {
        Self {
            id: msg.id,
            role: format!("{:?}", msg.role),
            content: msg.get_text().cloned().unwrap_or_default(),
            created_at: msg.created_at,
        }
    }
}

/// Health check endpoint
pub async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// List all sessions
pub async fn list_sessions(State(state): State<AppState>) -> impl IntoResponse {
    match state.session_manager.list_sessions().await {
        Ok(sessions) => Json(sessions).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Create a new session
#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub title: Option<String>,
    pub settings: Option<SessionSettings>,
}

pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let title = req.title.unwrap_or_else(|| "New Session".to_string());

    let session = Session::new(&title);

    let session = if let Some(settings) = req.settings {
        session.with_settings(settings)
    } else {
        session
    };

    match state.session_manager.create_session(session).await {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": id }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Get a session
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.session_manager.get_session(id).await {
        Ok(session) => Json(session).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Delete a session
pub async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.session_manager.close_session(id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Get messages for a session
pub async fn get_messages(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.session_manager.get_history(id).await {
        Ok(history) => {
            let messages: Vec<ClientMessage> = history.messages.into_iter().map(Into::into).collect();
            Json(messages).into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Send a message to a session
#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
}

pub async fn send_message(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let message = Message::user_text(req.content);

    match state.session_manager.add_message(id, message).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Update session settings
pub async fn update_settings(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(settings): Json<SessionSettings>,
) -> impl IntoResponse {
    match state.session_manager.update_settings(id, settings).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// List available tools
pub async fn list_tools(State(state): State<AppState>) -> impl IntoResponse {
    let tools = state
        .tool_registry
        .list()
        .into_iter()
        .map(|t| serde_json::json!({
            "id": t.id,
            "name": t.name,
            "description": t.description,
            "parameters": t.parameters,
            "requires_permission": t.requires_permission,
            "is_destructive": t.is_destructive,
        }))
        .collect::<Vec<_>>();

    Json(tools).into_response()
}

// ==================== Project Handlers ====================

/// List all projects
pub async fn list_projects(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query(
        "SELECT id, name, path, type, mode, description, backend_test, lint_cmd, frontend_test,
                auto_promote, is_active, status, created_at
        FROM projects
        ORDER BY created_at DESC"
    )
    .fetch_all(state.session_manager.get_db())
    .await
    {
        Ok(rows) => {
            let projects_list: Vec<ProjectResponse> = rows
                .into_iter()
                .map(|row| {
                    let id: Uuid = row.get("id");
                    let name: String = row.get("name");
                    let path: String = row.get("path");
                    let r#type: String = row.get("type");
                    let mode: String = row.get("mode");
                    let description: String = row.get("description");
                    let backend_test: Option<String> = row.get("backend_test");
                    let lint_cmd: Option<String> = row.get("lint_cmd");
                    let frontend_test: Option<String> = row.get("frontend_test");
                    let auto_promote: bool = row.get("auto_promote");
                    let is_active: bool = row.get("is_active");
                    let status: String = row.get("status");
                    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

                    ProjectResponse {
                        id,
                        name,
                        path,
                        r#type,
                        mode,
                        description,
                        backend_test,
                        lint_cmd,
                        frontend_test,
                        auto_promote,
                        is_active,
                        status,
                        created_at,
                        run_count: 0,
                    }
                })
                .collect();
            Json(projects_list).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Request to create a project
#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub backend_test: Option<String>,
    #[serde(default)]
    pub lint_cmd: Option<String>,
    #[serde(default)]
    pub frontend_test: Option<String>,
    #[serde(default)]
    pub auto_promote: Option<bool>,
    #[serde(default)]
    pub settings: Option<String>,
}

/// Create a new project
pub async fn create_project(
    State(state): State<AppState>,
    Json(req): Json<CreateProjectRequest>,
) -> impl IntoResponse {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let r#type = req.r#type.unwrap_or_else(|| "directory".to_string());
    let mode = req.mode.unwrap_or_else(|| "host".to_string());
    let description = req.description.unwrap_or_default();
    let auto_promote = req.auto_promote.unwrap_or(false);

    match sqlx::query(
        r#"
        INSERT INTO projects (id, name, path, type, mode, description, backend_test, lint_cmd,
                             frontend_test, auto_promote, is_active, status, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.path)
    .bind(&r#type)
    .bind(&mode)
    .bind(&description)
    .bind(&req.backend_test)
    .bind(&req.lint_cmd)
    .bind(&req.frontend_test)
    .bind(auto_promote)
    .bind(true)
    .bind("ready")
    .bind(now)
    .bind(now)
    .execute(state.session_manager.get_db())
    .await
    {
        Ok(_) => {
            let project = ProjectResponse {
                id,
                name: req.name,
                path: req.path,
                r#type,
                mode,
                description,
                backend_test: req.backend_test,
                lint_cmd: req.lint_cmd,
                frontend_test: req.frontend_test,
                auto_promote,
                is_active: true,
                status: "ready".to_string(),
                created_at: now,
                run_count: 0,
            };
            (StatusCode::CREATED, Json(project)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Get a project
pub async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let db = state.session_manager.get_db();

    // Get project
    let project = match sqlx::query(
        "SELECT id, name, path, type, mode, description, backend_test, lint_cmd, frontend_test,
                auto_promote, is_active, status, created_at
         FROM projects WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(db)
    .await
    {
        Ok(Some(row)) => {
            let id: Uuid = row.get("id");
            let name: String = row.get("name");
            let path: String = row.get("path");
            let r#type: String = row.get("type");
            let mode: String = row.get("mode");
            let description: String = row.get("description");
            let backend_test: Option<String> = row.get("backend_test");
            let lint_cmd: Option<String> = row.get("lint_cmd");
            let frontend_test: Option<String> = row.get("frontend_test");
            let auto_promote: bool = row.get("auto_promote");
            let is_active: bool = row.get("is_active");
            let status: String = row.get("status");
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

            ProjectResponse {
                id,
                name,
                path,
                r#type,
                mode,
                description,
                backend_test,
                lint_cmd,
                frontend_test,
                auto_promote,
                is_active,
                status,
                created_at,
                run_count: 0,
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Project not found" })),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("{}", e) })),
            )
                .into_response()
        }
    };

    // Get run count
    let run_count = match sqlx::query("SELECT COUNT(*) FROM runs WHERE project_id = ?")
        .bind(id)
        .fetch_one(db)
        .await
    {
        Ok(row) => {
            let count: i64 = row.get(0);
            count
        }
        Err(_) => 0,
    };

    let response = ProjectDetailResponse {
        project,
        run_count,
    };

    Json(response).into_response()
}

/// Update project request
#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub path: Option<String>,
    pub r#type: Option<String>,
    pub mode: Option<String>,
    pub description: Option<String>,
    pub backend_test: Option<String>,
    pub lint_cmd: Option<String>,
    pub frontend_test: Option<String>,
    pub auto_promote: Option<bool>,
    pub is_active: Option<bool>,
    pub status: Option<String>,
}

/// Update a project
pub async fn update_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateProjectRequest>,
) -> impl IntoResponse {
    let db = state.session_manager.get_db();

    // Update each field individually
    let mut updated = false;

    if let Some(ref name) = req.name {
        if sqlx::query("UPDATE projects SET name = ? WHERE id = ?")
            .bind(name)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(ref path) = req.path {
        if sqlx::query("UPDATE projects SET path = ? WHERE id = ?")
            .bind(path)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(ref r#type) = req.r#type {
        if sqlx::query("UPDATE projects SET type = ? WHERE id = ?")
            .bind(r#type)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(ref mode) = req.mode {
        if sqlx::query("UPDATE projects SET mode = ? WHERE id = ?")
            .bind(mode)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(ref description) = req.description {
        if sqlx::query("UPDATE projects SET description = ? WHERE id = ?")
            .bind(description)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(ref backend_test) = req.backend_test {
        if sqlx::query("UPDATE projects SET backend_test = ? WHERE id = ?")
            .bind(backend_test)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(ref lint_cmd) = req.lint_cmd {
        if sqlx::query("UPDATE projects SET lint_cmd = ? WHERE id = ?")
            .bind(lint_cmd)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(ref frontend_test) = req.frontend_test {
        if sqlx::query("UPDATE projects SET frontend_test = ? WHERE id = ?")
            .bind(frontend_test)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(auto_promote) = req.auto_promote {
        if sqlx::query("UPDATE projects SET auto_promote = ? WHERE id = ?")
            .bind(auto_promote)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(is_active) = req.is_active {
        if sqlx::query("UPDATE projects SET is_active = ? WHERE id = ?")
            .bind(is_active)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if let Some(ref status) = req.status {
        if sqlx::query("UPDATE projects SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(db)
            .await
            .is_ok()
        {
            updated = true;
        }
    }

    if !updated {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "No fields to update" })),
        )
            .into_response();
    }

    // Fetch and return updated project
    match sqlx::query(
        "SELECT id, name, path, type, mode, description, backend_test, lint_cmd, frontend_test,
                auto_promote, is_active, status, created_at
         FROM projects WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(db)
    .await
    {
        Ok(Some(row)) => {
            let id: Uuid = row.get("id");
            let name: String = row.get("name");
            let path: String = row.get("path");
            let r#type: String = row.get("type");
            let mode: String = row.get("mode");
            let description: String = row.get("description");
            let backend_test: Option<String> = row.get("backend_test");
            let lint_cmd: Option<String> = row.get("lint_cmd");
            let frontend_test: Option<String> = row.get("frontend_test");
            let auto_promote: bool = row.get("auto_promote");
            let is_active: bool = row.get("is_active");
            let status: String = row.get("status");
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");

            let project = ProjectResponse {
                id,
                name,
                path,
                r#type,
                mode,
                description,
                backend_test,
                lint_cmd,
                frontend_test,
                auto_promote,
                is_active,
                status,
                created_at,
                run_count: 0,
            };

            (StatusCode::OK, Json(project)).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Project not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Delete a project
pub async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let db = state.session_manager.get_db();

    // Delete associated runs
    let _ = sqlx::query("DELETE FROM runs WHERE project_id = ?")
        .bind(id)
        .execute(db)
        .await;

    // Delete the project
    match sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(db)
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

// Response types
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub mode: String,
    pub description: String,
    pub backend_test: Option<String>,
    pub lint_cmd: Option<String>,
    pub frontend_test: Option<String>,
    pub auto_promote: bool,
    pub is_active: bool,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub run_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectDetailResponse {
    #[serde(flatten)]
    pub project: ProjectResponse,
    pub run_count: i64,
}

// ==================== Run Handlers ====================

/// List runs for a project
pub async fn list_runs(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.run_manager.list_runs(Some(project_id)).await {
        Ok(runs) => Json(runs).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Request to create a run
#[derive(Deserialize)]
pub struct CreateRunRequest {
    pub project_id: Uuid,
    pub description: String,
    pub branch: Option<String>,
    pub commit_hash: Option<String>,
    pub priority: Option<i32>,
    pub tags: Option<Vec<String>>,
}

/// Create a new run
pub async fn create_run(
    State(state): State<AppState>,
    Path(_project_id): Path<Uuid>,
    Json(req): Json<CreateRunRequest>,
) -> impl IntoResponse {
    let config = RunConfig {
        project_id: req.project_id,
        description: req.description,
        branch: req.branch,
        commit_hash: req.commit_hash,
        priority: req.priority.unwrap_or(0),
        tags: req.tags.unwrap_or_default(),
        metadata: std::collections::HashMap::new(),
    };

    match state.run_manager.create_run(config).await {
        Ok(run_id) => (StatusCode::CREATED, Json(serde_json::json!({ "id": run_id }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Get a run
pub async fn get_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.run_manager.get_run(id).await {
        Ok(run) => Json(run).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Cancel a run
pub async fn cancel_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.run_manager.cancel_run(id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// List tasks for a run
pub async fn list_tasks(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.run_manager.list_tasks(id).await {
        Ok(tasks) => Json(tasks).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// List artifacts for a run
pub async fn list_artifacts(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.run_manager.list_artifacts(id).await {
        Ok(artifacts) => Json(artifacts).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// List audit events for a run
pub async fn list_audit_events(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match state.run_manager.get_audit_events(Some(id)).await {
        Ok(events) => Json(events).into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("{}", e) })),
        )
            .into_response(),
    }
}

/// Get run statistics
pub async fn get_stats(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.run_manager.get_stats().await;
    Json(stats).into_response()
}

/// Serve the index page
pub async fn index() -> impl IntoResponse {
    let html = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>YowCode - AI-Powered Code Assistant</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0d1117; color: #c9d1d9; }
        .container { max-width: 1200px; margin: 0 auto; padding: 20px; height: 100vh; display: flex; flex-direction: column; }
        .header { display: flex; justify-content: space-between; align-items: center; padding-bottom: 20px; border-bottom: 1px solid #30363d; }
        .header h1 { font-size: 24px; color: #58a6ff; }
        .sessions { display: flex; gap: 10px; margin-bottom: 20px; }
        .session-btn { padding: 8px 16px; background: #21262d; border: 1px solid #30363d; color: #c9d1d9; cursor: pointer; border-radius: 6px; }
        .session-btn:hover { background: #30363d; }
        .session-btn.active { background: #238636; border-color: #238636; }
        .chat-container { flex: 1; display: flex; flex-direction: column; overflow: hidden; }
        .messages { flex: 1; overflow-y: auto; padding: 20px; }
        .message { margin-bottom: 20px; padding: 15px; border-radius: 8px; max-width: 80%; }
        .message.user { background: #21262d; margin-left: auto; }
        .message.assistant { background: #1f6feb; color: white; margin-right: auto; }
        .message.system { background: #8957e5; color: white; margin-right: auto; font-style: italic; }
        .message-role { font-size: 12px; opacity: 0.7; margin-bottom: 5px; }
        .input-area { display: flex; gap: 10px; padding: 20px; background: #0d1117; border-top: 1px solid #30363d; }
        .input-area textarea { flex: 1; background: #21262d; border: 1px solid #30363d; color: #c9d1d9; padding: 12px; border-radius: 6px; resize: none; font-family: inherit; }
        .input-area button { padding: 12px 24px; background: #238636; border: none; color: white; cursor: pointer; border-radius: 6px; font-weight: 600; }
        .input-area button:hover { background: #2ea043; }
        .input-area button:disabled { background: #30363d; cursor: not-allowed; }
        .status { padding: 10px; text-align: center; color: #8b949e; font-size: 14px; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>YowCode</h1>
            <button id="newSession" class="session-btn">+ New Session</button>
        </div>
        <div class="sessions" id="sessions"></div>
        <div class="chat-container">
            <div class="messages" id="messages"></div>
            <div class="status" id="status">Ready</div>
            <div class="input-area">
                <textarea id="input" placeholder="Type your message... (Shift+Enter for new line)" rows="3"></textarea>
                <button id="send">Send</button>
            </div>
        </div>
    </div>
    <script>
        let ws;
        let currentSession = null;

        async function connect() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            ws = new WebSocket(`${protocol}//${window.location.host}/ws`);

            ws.onopen = () => {
                console.log('Connected');
                loadSessions();
            };

            ws.onmessage = (event) => {
                const msg = JSON.parse(event.data);
                handleMessage(msg);
            };

            ws.onclose = () => {
                console.log('Disconnected, reconnecting...');
                setTimeout(connect, 1000);
            };
        }

        function handleMessage(msg) {
            switch (msg.type) {
                case 'SessionCreated':
                    addSession(msg.session_id, msg.title);
                    joinSession(msg.session_id);
                    break;
                case 'SessionJoined':
                    currentSession = msg.session_id;
                    renderMessages(msg.messages);
                    break;
                case 'ChatResponse':
                    addMessage('assistant', msg.content);
                    break;
                case 'StatusUpdate':
                    document.getElementById('status').textContent = msg.status;
                    break;
                case 'ToolCall':
                    addMessage('system', `Running: ${msg.tool_name}`);
                    break;
                case 'Error':
                    console.error(msg.message);
                    break;
            }
        }

        async function loadSessions() {
            const response = await fetch('/api/sessions');
            const sessions = await response.json();
            const container = document.getElementById('sessions');
            container.innerHTML = '';
            sessions.forEach(session => {
                addSession(session.id, session.title, session.is_active);
            });
        }

        function addSession(id, title, isActive = true) {
            const container = document.getElementById('sessions');
            const btn = document.createElement('button');
            btn.className = `session-btn ${isActive ? 'active' : ''}`;
            btn.textContent = title;
            btn.onclick = () => joinSession(id);
            container.appendChild(btn);
        }

        function joinSession(id) {
            currentSession = id;
            ws.send(JSON.stringify({ type: 'JoinSession', session_id: id }));
            updateActiveSession(id);
        }

        function updateActiveSession(id) {
            document.querySelectorAll('.session-btn').forEach(btn => {
                btn.classList.remove('active');
            });
            // Find and activate the clicked button (simplified)
        }

        function renderMessages(messages) {
            const container = document.getElementById('messages');
            container.innerHTML = '';
            messages.forEach(msg => {
                addMessage(msg.role.toLowerCase(), msg.content);
            });
        }

        function addMessage(role, content) {
            const container = document.getElementById('messages');
            const div = document.createElement('div');
            div.className = `message ${role}`;
            div.innerHTML = `
                <div class="message-role">${role}</div>
                <div>${escapeHtml(content)}</div>
            `;
            container.appendChild(div);
            container.scrollTop = container.scrollHeight;
        }

        function escapeHtml(text) {
            const div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML.replace(/\n/g, '<br>');
        }

        function sendMessage() {
            const input = document.getElementById('input');
            const content = input.value.trim();
            if (!content || !currentSession) return;

            addMessage('user', content);
            input.value = '';

            ws.send(JSON.stringify({
                type: 'ChatRequest',
                session_id: currentSession,
                message: content
            }));
        }

        function createSession() {
            ws.send(JSON.stringify({
                type: 'CreateSession',
                title: 'New Session'
            }));
        }

        document.getElementById('send').onclick = sendMessage;
        document.getElementById('input').onkeydown = (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                sendMessage();
            }
        };
        document.getElementById('newSession').onclick = createSession;

        connect();
    </script>
</body>
</html>
    "#;

    axum::response::Html(html).into_response()
}

/// Projects page
pub async fn projects_page() -> impl IntoResponse {
    let html = include_str!("../static/projects.html");
    axum::response::Html(html).into_response()
}

/// Serve static files
pub async fn static_files(Path(path): Path<String>) -> impl IntoResponse {
    let path_str = path.as_str();

    // Security: prevent directory traversal
    if path_str.contains("..") {
        return (StatusCode::FORBIDDEN, "Access denied").into_response();
    }

    // Try to serve from static directory
    let static_path = format!("../static/{}", path_str.trim_start_matches('/'));

    if let Ok(content) = tokio::fs::read_to_string(&static_path).await {
        // Simple MIME type detection
        let mime_type = if static_path.ends_with(".html") {
            "text/html"
        } else if static_path.ends_with(".css") {
            "text/css"
        } else if static_path.ends_with(".js") {
            "application/javascript"
        } else {
            "text/plain"
        };

        return match axum::http::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", mime_type)
            .body(content)
        {
            Ok(response) => response.into_response(),
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to build response").into_response(),
        };
    }

    (StatusCode::NOT_FOUND, "File not found").into_response()
}

