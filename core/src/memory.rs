//! Memory and persistence system for YowCode
//!
//! This module provides long-term memory and persistence capabilities:
//! - User preferences and settings persistence
//! - Conversation history and context memory
//! - Knowledge base and fact storage
//! - Cross-session learning and adaptation

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use std::collections::HashMap;
use uuid::Uuid;

/// Memory entry types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    /// User preference or setting
    Preference,
    /// Fact learned from conversation
    Fact,
    /// User context information
    Context,
    /// Pattern or habit learned
    Pattern,
    /// Custom memory type
    Custom(String),
}

/// Memory entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: Uuid,
    pub memory_type: MemoryType,
    pub key: String,
    pub value: String,
    pub metadata: HashMap<String, String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub access_count: i64,
    pub last_accessed: Option<chrono::DateTime<chrono::Utc>>,
}

impl MemoryEntry {
    /// Create a new memory entry
    pub fn new(memory_type: MemoryType, key: String, value: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            memory_type,
            key,
            value,
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            access_count: 0,
            last_accessed: None,
        }
    }

    /// Add metadata to the entry
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Update the value and timestamp
    pub fn update(&mut self, value: String) {
        self.value = value;
        self.updated_at = chrono::Utc::now();
    }

    /// Record access to this entry
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Some(chrono::Utc::now());
    }
}

/// User profile with persistent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: Uuid,
    pub preferences: HashMap<String, String>,
    pub context: HashMap<String, String>,
    pub statistics: UserStatistics,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// User usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStatistics {
    pub total_sessions: i64,
    pub total_messages: i64,
    pub total_tool_calls: i64,
    pub most_used_tools: Vec<(String, i64)>,
    pub most_used_agents: Vec<(String, i64)>,
    pub average_session_length_seconds: i64,
}

impl Default for UserStatistics {
    fn default() -> Self {
        Self {
            total_sessions: 0,
            total_messages: 0,
            total_tool_calls: 0,
            most_used_tools: Vec::new(),
            most_used_agents: Vec::new(),
            average_session_length_seconds: 0,
        }
    }
}

impl UserProfile {
    /// Create a new user profile
    pub fn new() -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            preferences: HashMap::new(),
            context: HashMap::new(),
            statistics: UserStatistics::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Set a preference
    pub fn set_preference(&mut self, key: String, value: String) {
        self.preferences.insert(key, value);
        self.updated_at = chrono::Utc::now();
    }

    /// Get a preference
    pub fn get_preference(&self, key: &str) -> Option<&String> {
        self.preferences.get(key)
    }

    /// Set context information
    pub fn set_context(&mut self, key: String, value: String) {
        self.context.insert(key, value);
        self.updated_at = chrono::Utc::now();
    }

    /// Get context information
    pub fn get_context(&self, key: &str) -> Option<&String> {
        self.context.get(key)
    }
}

/// Memory store for persistent storage
#[derive(Clone)]
pub struct MemoryStore {
    db: Pool<Sqlite>,
    user_id: Uuid,
}

impl MemoryStore {
    /// Create a new memory store
    pub async fn new(db: Pool<Sqlite>, user_id: Uuid) -> Result<Self> {
        // Initialize memory tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                memory_type TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                metadata TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                access_count INTEGER DEFAULT 0,
                last_accessed TEXT
            )
            "#,
        )
        .execute(&db)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS user_profiles (
                id TEXT PRIMARY KEY,
                preferences TEXT,
                context TEXT,
                statistics TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&db)
        .await?;

        // Create indexes
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_user_id ON memory_entries(user_id)")
            .execute(&db)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_type ON memory_entries(memory_type)")
            .execute(&db)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_memory_key ON memory_entries(key)")
            .execute(&db)
            .await?;

        Ok(Self { db, user_id })
    }

    /// Store a memory entry
    pub async fn store(&self, entry: MemoryEntry) -> Result<()> {
        let metadata_json = serde_json::to_string(&entry.metadata)?;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO memory_entries
            (id, user_id, memory_type, key, value, metadata, created_at, updated_at, access_count, last_accessed)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
        )
        .bind(entry.id)
        .bind(self.user_id)
        .bind(serde_json::to_string(&entry.memory_type)?)
        .bind(&entry.key)
        .bind(&entry.value)
        .bind(metadata_json)
        .bind(entry.created_at)
        .bind(entry.updated_at)
        .bind(entry.access_count)
        .bind(entry.last_accessed)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Retrieve a memory entry by key
    pub async fn get(&self, key: &str) -> Result<Option<MemoryEntry>> {
        let row = sqlx::query(
            "SELECT * FROM memory_entries WHERE user_id = ? AND key = ?"
        )
        .bind(self.user_id)
        .bind(key)
        .fetch_optional(&self.db)
        .await?;

        match row {
            Some(row) => {
                let id: Uuid = row.get("id");
                let memory_type_str: String = row.get("memory_type");
                let memory_type: MemoryType = serde_json::from_str(&memory_type_str)?;
                let entry_key: String = row.get("key");
                let value: String = row.get("value");
                let metadata_str: String = row.get("metadata");
                let metadata: HashMap<String, String> = serde_json::from_str(&metadata_str)?;
                let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
                let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
                let access_count: i64 = row.get("access_count");
                let last_accessed: Option<String> = row.get("last_accessed");
                let _last_accessed = match last_accessed {
                    Some(date_str) => {
                        Some(chrono::DateTime::parse_from_rfc3339(&date_str)
                            .map_err(|e| Error::Other(format!("Invalid date format: {}", e)))?
                            .with_timezone(&chrono::Utc))
                    }
                    None => None,
                };

                // Update access count
                sqlx::query("UPDATE memory_entries SET access_count = access_count + 1, last_accessed = ? WHERE id = ?")
                    .bind(chrono::Utc::now())
                    .bind(id)
                    .execute(&self.db)
                    .await?;

                Ok(Some(MemoryEntry {
                    id,
                    memory_type,
                    key: entry_key,
                    value,
                    metadata,
                    created_at,
                    updated_at,
                    access_count: access_count + 1,
                    last_accessed: Some(chrono::Utc::now()),
                }))
            }
            None => Ok(None),
        }
    }

    /// Retrieve all memories of a specific type
    pub async fn get_by_type(&self, memory_type: MemoryType) -> Result<Vec<MemoryEntry>> {
        let type_str = serde_json::to_string(&memory_type)?;

        let rows = sqlx::query(
            "SELECT * FROM memory_entries WHERE user_id = ? AND memory_type = ?"
        )
        .bind(self.user_id)
        .bind(type_str)
        .fetch_all(&self.db)
        .await?;

        let mut entries = Vec::new();
        for row in rows {
            let id: Uuid = row.get("id");
            let memory_type_str: String = row.get("memory_type");
            let memory_type: MemoryType = serde_json::from_str(&memory_type_str)?;
            let key: String = row.get("key");
            let value: String = row.get("value");
            let metadata_str: String = row.get("metadata");
            let metadata: HashMap<String, String> = serde_json::from_str(&metadata_str)?;
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
            let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
            let access_count: i64 = row.get("access_count");
            let last_accessed: Option<String> = row.get("last_accessed");
            let last_accessed = match last_accessed {
                Some(date_str) => {
                    Some(chrono::DateTime::parse_from_rfc3339(&date_str)
                        .map_err(|e| Error::Other(format!("Invalid date format: {}", e)))?
                        .with_timezone(&chrono::Utc))
                }
                None => None,
            };

            entries.push(MemoryEntry {
                id,
                memory_type,
                key,
                value,
                metadata,
                created_at,
                updated_at,
                access_count,
                last_accessed,
            });
        }

        Ok(entries)
    }

    /// Search memories by key pattern
    pub async fn search(&self, pattern: &str) -> Result<Vec<MemoryEntry>> {
        let search_pattern = format!("%{}%", pattern);

        let rows = sqlx::query(
            "SELECT * FROM memory_entries WHERE user_id = ? AND key LIKE ?"
        )
        .bind(self.user_id)
        .bind(search_pattern)
        .fetch_all(&self.db)
        .await?;

        let mut entries = Vec::new();
        for row in rows {
            let id: Uuid = row.get("id");
            let memory_type_str: String = row.get("memory_type");
            let memory_type: MemoryType = serde_json::from_str(&memory_type_str)?;
            let key: String = row.get("key");
            let value: String = row.get("value");
            let metadata_str: String = row.get("metadata");
            let metadata: HashMap<String, String> = serde_json::from_str(&metadata_str)?;
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
            let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
            let access_count: i64 = row.get("access_count");
            let last_accessed: Option<String> = row.get("last_accessed");
            let last_accessed = match last_accessed {
                Some(date_str) => {
                    Some(chrono::DateTime::parse_from_rfc3339(&date_str)
                        .map_err(|e| Error::Other(format!("Invalid date format: {}", e)))?
                        .with_timezone(&chrono::Utc))
                }
                None => None,
            };

            entries.push(MemoryEntry {
                id,
                memory_type,
                key,
                value,
                metadata,
                created_at,
                updated_at,
                access_count,
                last_accessed,
            });
        }

        Ok(entries)
    }

    /// Delete a memory entry
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM memory_entries WHERE user_id = ? AND key = ?")
            .bind(self.user_id)
            .bind(key)
            .execute(&self.db)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get or create user profile
    pub async fn get_profile(&self) -> Result<UserProfile> {
        let row = sqlx::query("SELECT * FROM user_profiles WHERE id = ?")
            .bind(self.user_id)
            .fetch_optional(&self.db)
            .await?;

        match row {
            Some(row) => {
                let preferences_str: String = row.get("preferences");
                let context_str: String = row.get("context");
                let statistics_str: String = row.get("statistics");
                let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
                let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

                Ok(UserProfile {
                    id: self.user_id,
                    preferences: serde_json::from_str(&preferences_str)?,
                    context: serde_json::from_str(&context_str)?,
                    statistics: serde_json::from_str(&statistics_str)?,
                    created_at,
                    updated_at,
                })
            }
            None => {
                // Create new profile
                let profile = UserProfile::new();
                let preferences_json = serde_json::to_string(&profile.preferences)?;
                let context_json = serde_json::to_string(&profile.context)?;
                let statistics_json = serde_json::to_string(&profile.statistics)?;

                sqlx::query(
                    r#"
                    INSERT INTO user_profiles (id, preferences, context, statistics, created_at, updated_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    "#,
                )
                .bind(profile.id)
                .bind(preferences_json)
                .bind(context_json)
                .bind(statistics_json)
                .bind(profile.created_at)
                .bind(profile.updated_at)
                .execute(&self.db)
                .await?;

                Ok(profile)
            }
        }
    }

    /// Update user profile
    pub async fn update_profile(&self, profile: &UserProfile) -> Result<()> {
        let preferences_json = serde_json::to_string(&profile.preferences)?;
        let context_json = serde_json::to_string(&profile.context)?;
        let statistics_json = serde_json::to_string(&profile.statistics)?;

        sqlx::query(
            r#"
            UPDATE user_profiles
            SET preferences = ?, context = ?, statistics = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(preferences_json)
        .bind(context_json)
        .bind(statistics_json)
        .bind(chrono::Utc::now())
        .bind(profile.id)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Get all memories for export/backup
    pub async fn export_all(&self) -> Result<Vec<MemoryEntry>> {
        let rows = sqlx::query("SELECT * FROM memory_entries WHERE user_id = ?")
            .bind(self.user_id)
            .fetch_all(&self.db)
            .await?;

        let mut entries = Vec::new();
        for row in rows {
            let id: Uuid = row.get("id");
            let memory_type_str: String = row.get("memory_type");
            let memory_type: MemoryType = serde_json::from_str(&memory_type_str)?;
            let key: String = row.get("key");
            let value: String = row.get("value");
            let metadata_str: String = row.get("metadata");
            let metadata: HashMap<String, String> = serde_json::from_str(&metadata_str)?;
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
            let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");
            let access_count: i64 = row.get("access_count");
            let last_accessed: Option<String> = row.get("last_accessed");
            let last_accessed = match last_accessed {
                Some(date_str) => {
                    Some(chrono::DateTime::parse_from_rfc3339(&date_str)
                        .map_err(|e| Error::Other(format!("Invalid date format: {}", e)))?
                        .with_timezone(&chrono::Utc))
                }
                None => None,
            };

            entries.push(MemoryEntry {
                id,
                memory_type,
                key,
                value,
                metadata,
                created_at,
                updated_at,
                access_count,
                last_accessed,
            });
        }

        Ok(entries)
    }

    /// Import memories from backup
    pub async fn import(&self, entries: Vec<MemoryEntry>) -> Result<usize> {
        let mut imported = 0;

        for entry in entries {
            if self.store(entry).await.is_ok() {
                imported += 1;
            }
        }

        Ok(imported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_entry_creation() {
        let entry = MemoryEntry::new(
            MemoryType::Preference,
            "theme".to_string(),
            "dark".to_string(),
        );

        assert_eq!(entry.key, "theme");
        assert_eq!(entry.value, "dark");
        assert_eq!(entry.access_count, 0);
    }

    #[test]
    fn test_user_profile() {
        let mut profile = UserProfile::new();
        profile.set_preference("language".to_string(), "rust".to_string());

        assert_eq!(
            profile.get_preference("language"),
            Some(&"rust".to_string())
        );
    }
}
