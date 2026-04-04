use crate::error::{Error, Result};
use crate::message::{ChatHistory, Message};
use crate::types::{PermissionMode, Project};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

/// Settings for a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSettings {
    pub permission_mode: PermissionMode,
    pub max_context_tokens: u32,
    pub theme: Option<String>,
    pub project_id: Option<Uuid>,
    pub interface_mode: crate::types::InterfaceMode,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            permission_mode: PermissionMode::Default,
            max_context_tokens: 200000,
            theme: None,
            project_id: None,
            interface_mode: crate::types::InterfaceMode::Both,
        }
    }
}

/// A chat session that can be shared between CLI and web
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub settings: SessionSettings,
    pub is_active: bool,
    pub current_directory: Option<String>,
}

impl Session {
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            created_at: now,
            updated_at: now,
            settings: SessionSettings::default(),
            is_active: true,
            current_directory: None,
        }
    }

    pub fn with_settings(mut self, settings: SessionSettings) -> Self {
        self.settings = settings;
        self
    }

    pub fn with_current_directory(mut self, dir: impl Into<String>) -> Self {
        self.current_directory = Some(dir.into());
        self
    }
}

/// Session manager that handles persistence and broadcasting
pub struct SessionManager {
    db: Pool<Sqlite>,
    sessions: Arc<RwLock<HashMap<Uuid, SessionState>>>,
    projects: Arc<RwLock<HashMap<Uuid, Project>>>,
    tx: broadcast::Sender<SessionEvent>,
}

/// In-memory state for an active session
#[derive(Clone)]
struct SessionState {
    session: Session,
    history: ChatHistory,
    context: Context,
}

/// Session context (file system, symbols, etc.)
#[derive(Debug, Clone)]
pub struct Context {
    pub current_directory: PathBuf,
    pub environment: HashMap<String, String>,
    pub git_branch: Option<String>,
    pub git_remote: Option<String>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            current_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            environment: std::env::vars().collect(),
            git_branch: None,
            git_remote: None,
        }
    }
}

/// Events that can be broadcast about a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    Message { session_id: Uuid, message: Message },
    Status { session_id: Uuid, status: String },
    ToolCall { session_id: Uuid, tool_name: String, status: String },
    SessionCreated { session_id: Uuid },
    SessionClosed { session_id: Uuid },
}

impl SessionManager {
    pub fn new(db: Pool<Sqlite>) -> Self {
        let (tx, _) = broadcast::channel(1000);
        Self {
            db,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            tx,
        }
    }

    /// Subscribe to session events
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.tx.subscribe()
    }

    /// Create a new session
    pub async fn create_session(&self, session: Session) -> Result<Uuid> {
        let id = session.id;
        let settings_json = serde_json::to_string(&session.settings)?;

        // Persist to database
        sqlx::query(
            r#"
            INSERT INTO sessions (id, title, created_at, updated_at, settings, is_active, current_directory)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(id)
        .bind(&session.title)
        .bind(session.created_at)
        .bind(session.updated_at)
        .bind(&settings_json)
        .bind(session.is_active)
        .bind(&session.current_directory)
        .execute(&self.db)
        .await?;

        // Add to in-memory state
        let state = SessionState {
            session,
            history: ChatHistory::new(id),
            context: Context::default(),
        };
        self.sessions.write().await.insert(id, state);

        // Broadcast event
        let _ = self.tx.send(SessionEvent::SessionCreated { session_id: id });

        Ok(id)
    }

    /// Get a session
    pub async fn get_session(&self, id: Uuid) -> Result<Session> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&id)
            .map(|state| state.session.clone())
            .ok_or_else(|| Error::SessionNotFound(id))
    }

    /// Get active sessions
    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.values().map(|s| s.session.clone()).collect())
    }

    /// Add a message to a session
    pub async fn add_message(&self, session_id: Uuid, message: Message) -> Result<()> {
        // Add to in-memory history
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(&session_id) {
            state.history.add_message(message.clone());
            state.session.updated_at = Utc::now();

            // Persist message to database
            let role_json = serde_json::to_string(&message.role)?;
            let content_json = serde_json::to_string(&message.content)?;
            let metadata_json: Option<String> = message.metadata.as_ref().map(|m| serde_json::to_string(m).unwrap());

            sqlx::query(
                r#"
                INSERT INTO messages (id, session_id, role, content, created_at, metadata)
                VALUES (?, ?, ?, ?, ?, ?)
                "#
            )
            .bind(message.id)
            .bind(session_id)
            .bind(&role_json)
            .bind(&content_json)
            .bind(message.created_at)
            .bind(metadata_json)
            .execute(&self.db)
            .await?;

            // Broadcast event
            let _ = self.tx.send(SessionEvent::Message {
                session_id,
                message,
            });

            Ok(())
        } else {
            Err(Error::SessionNotFound(session_id))
        }
    }

    /// Get chat history for a session
    pub async fn get_history(&self, session_id: Uuid) -> Result<ChatHistory> {
        let sessions = self.sessions.read().await;
        sessions
            .get(&session_id)
            .map(|state| state.history.clone())
            .ok_or_else(|| Error::SessionNotFound(session_id))
    }

    /// Update session settings
    pub async fn update_settings(&self, session_id: Uuid, settings: SessionSettings) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(state) = sessions.get_mut(&session_id) {
            state.session.settings = settings.clone();
            state.session.updated_at = Utc::now();

            let settings_json = serde_json::to_string(&settings)?;

            sqlx::query(
                r#"
                UPDATE sessions
                SET settings = ?, updated_at = ?
                WHERE id = ?
                "#
            )
            .bind(&settings_json)
            .bind(Utc::now())
            .bind(session_id)
            .execute(&self.db)
            .await?;

            Ok(())
        } else {
            Err(Error::SessionNotFound(session_id))
        }
    }

    /// Close a session
    pub async fn close_session(&self, session_id: Uuid) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(mut state) = sessions.remove(&session_id) {
            state.session.is_active = false;
            state.session.updated_at = Utc::now();

            sqlx::query("UPDATE sessions SET is_active = 0, updated_at = ? WHERE id = ?")
                .bind(Utc::now())
                .bind(session_id)
                .execute(&self.db)
                .await?;

            let _ = self.tx.send(SessionEvent::SessionClosed { session_id });
            Ok(())
        } else {
            Err(Error::SessionNotFound(session_id))
        }
    }

    // ==================== Project Management ====================

    /// Create a new project
    pub async fn create_project(&self, mut project: Project) -> Result<Uuid> {
        let id = project.id;

        // Persist to database
        sqlx::query(
            r#"
            INSERT INTO projects (id, name, path, description, execution_mode, container_image, env_vars, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(id)
        .bind(&project.name)
        .bind(&project.path)
        .bind(&project.description)
        .bind(serde_json::to_string(&project.execution_mode)?)
        .bind(&project.container_image)
        .bind(serde_json::to_string(&project.env_vars)?)
        .bind(project.created_at)
        .bind(project.updated_at)
        .execute(&self.db)
        .await?;

        // Add to in-memory state
        self.projects.write().await.insert(id, project.clone());

        Ok(id)
    }

    /// Get a project
    pub async fn get_project(&self, id: Uuid) -> Result<Project> {
        let projects = self.projects.read().await;
        projects
            .get(&id)
            .cloned()
            .ok_or_else(|| Error::Other(format!("Project {} not found", id)))
    }

    /// List all projects
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let projects = self.projects.read().await;
        Ok(projects.values().cloned().collect())
    }

    /// Update a project
    pub async fn update_project(&self, mut project: Project) -> Result<()> {
        let id = project.id;
        project.updated_at = Utc::now();

        // Update in database
        sqlx::query(
            r#"
            UPDATE projects
            SET name = ?, path = ?, description = ?, execution_mode = ?, container_image = ?, env_vars = ?, updated_at = ?
            WHERE id = ?
            "#
        )
        .bind(&project.name)
        .bind(&project.path)
        .bind(&project.description)
        .bind(serde_json::to_string(&project.execution_mode)?)
        .bind(&project.container_image)
        .bind(serde_json::to_string(&project.env_vars)?)
        .bind(project.updated_at)
        .bind(id)
        .execute(&self.db)
        .await?;

        // Update in-memory state
        self.projects.write().await.insert(id, project);

        Ok(())
    }

    /// Delete a project
    pub async fn delete_project(&self, id: Uuid) -> Result<()> {
        // Remove from in-memory state
        let mut projects = self.projects.write().await;
        if projects.remove(&id).is_some() {
            // Delete from database
            sqlx::query("DELETE FROM projects WHERE id = ?")
                .bind(id)
                .execute(&self.db)
                .await?;

            Ok(())
        } else {
            Err(Error::Other(format!("Project {} not found", id)))
        }
    }
}
