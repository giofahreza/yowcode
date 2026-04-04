use crate::error::Result;
use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};

/// Initialize the database connection and run migrations
pub async fn initialize_database(db_path: &str) -> Result<Pool<Sqlite>> {
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(db_path)
        .await?;

    // Run migrations
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            settings TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 1,
            current_directory TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            metadata TEXT,
            FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL,
            description TEXT,
            execution_mode TEXT NOT NULL DEFAULT 'Host',
            container_image TEXT,
            env_vars TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS runs (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            prompt TEXT NOT NULL,
            is_continuous INTEGER NOT NULL DEFAULT 0,
            max_cycles INTEGER,
            status TEXT NOT NULL DEFAULT 'Queued',
            created_at TEXT NOT NULL,
            started_at TEXT,
            completed_at TEXT,
            FOREIGN KEY (project_id) REFERENCES projects(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            description TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'Pending',
            owner TEXT,
            created_at TEXT NOT NULL,
            completed_at TEXT,
            FOREIGN KEY (run_id) REFERENCES runs(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS artifacts (
            id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            task_id TEXT,
            artifact_type TEXT NOT NULL,
            content TEXT NOT NULL,
            metadata TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (run_id) REFERENCES runs(id),
            FOREIGN KEY (task_id) REFERENCES tasks(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS audit_events (
            id TEXT PRIMARY KEY,
            run_id TEXT,
            event_type TEXT NOT NULL,
            data TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (run_id) REFERENCES runs(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Create indexes
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id)")
        .execute(&pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_runs_project_id ON runs(project_id)")
        .execute(&pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tasks_run_id ON tasks(run_id)")
        .execute(&pool)
        .await?;

    Ok(pool)
}

/// Clear all data (useful for testing)
pub async fn clear_database(pool: &Pool<Sqlite>) -> Result<()> {
    sqlx::query("DELETE FROM audit_events")
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM artifacts")
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM tasks")
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM runs")
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM messages")
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM sessions")
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM projects")
        .execute(pool)
        .await?;

    Ok(())
}
