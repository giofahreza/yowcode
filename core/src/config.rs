use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub ai: AIProviderConfig,
    pub server: ServerConfig,
    pub cli: CLIConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            ai: AIProviderConfig::default(),
            server: ServerConfig::default(),
            cli: CLIConfig::default(),
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: "~/.yowcode/yowcode.db".to_string(),
        }
    }
}

/// AI provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIProviderConfig {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for AIProviderConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".to_string(),
            api_key: String::new(),
            base_url: "https://api.anthropic.com/v1/messages".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 8192,
            temperature: 0.7,
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            cors_origins: vec!["http://localhost:3000".to_string()],
        }
    }
}

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CLIConfig {
    pub theme: String,
    pub editor: Option<String>,
    pub default_permission_mode: String,
}

impl Default for CLIConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            editor: None,
            default_permission_mode: "default".to_string(),
        }
    }
}

impl Config {
    /// Load configuration from a file
    pub async fn load(path: Option<PathBuf>) -> Result<Self> {
        let config_path = path.unwrap_or_else(|| {
            dirs::home_dir()
                .map(|p| p.join(".yowcode/config.toml"))
                .unwrap_or_else(|| PathBuf::from("config.toml"))
        });

        if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path).await?;
            let config: Config = toml::from_str(&content)
                .map_err(|e| crate::error::Error::InvalidConfiguration(e.to_string()))?;
            Ok(config)
        } else {
            // Check environment variables
            Ok(Self::from_env())
        }
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = Config::default();

        if let Ok(api_key) = std::env::var("YOWCODE_API_KEY") {
            config.ai.api_key = api_key;
        }

        if let Ok(base_url) = std::env::var("YOWCODE_BASE_URL") {
            config.ai.base_url = base_url;
        }

        if let Ok(model) = std::env::var("YOWCODE_MODEL") {
            config.ai.model = model;
        }

        if let Ok(db_path) = std::env::var("YOWCODE_DB_PATH") {
            config.database.path = db_path;
        }

        config
    }

    /// Save configuration to a file
    pub async fn save(&self, path: Option<PathBuf>) -> Result<()> {
        let config_path = path.unwrap_or_else(|| {
            dirs::home_dir()
                .map(|p| p.join(".yowcode/config.toml"))
                .unwrap_or_else(|| PathBuf::from("config.toml"))
        });

        // Create parent directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::error::Error::InvalidConfiguration(e.to_string()))?;

        tokio::fs::write(&config_path, content).await?;
        Ok(())
    }

    /// Expand home directory in paths
    pub fn expand_path(path: &str) -> PathBuf {
        if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(&path[2..]);
            }
        }
        PathBuf::from(path)
    }
}
