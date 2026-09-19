use std::path::PathBuf;

use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Postgres connection details entered during the setup wizard and persisted to settings.json
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String,
}

impl DbConfig {
    pub fn base_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}",
            self.user, self.password, self.host, self.port
        )
    }
}

/// Where to reach Ollama and how much context to budget
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub url: String,
    pub context_length: u64,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:11434".to_string(),
            context_length: 32768,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub database: Option<DbConfig>,
    #[serde(default)]
    pub ollama: OllamaConfig,
    pub jwt_secret: String,
}

impl AppConfig {
    fn fresh() -> Self {
        Self {
            database: None,
            ollama: OllamaConfig::default(),
            jwt_secret: generate_secret(),
        }
    }
}

/// Where `settings.json` lives, on every platform: `<exe_dir>/data/settings.json`, next to the binary
pub fn config_path() -> PathBuf {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join("data").join("settings.json")
}

/// Reads settings.json if it exists and parses
pub fn load() -> Option<AppConfig> {
    let path = config_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&raw) {
        Ok(config) => Some(config),
        Err(e) => {
            tracing::error!("failed to parse config at {}: {e}", path.display());
            None
        }
    }
}

/// Writes config to settings.json
pub fn save(config: &AppConfig) -> std::io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)
}

/// Loads the existing config, or creates+persists a fresh one on first run
pub fn load_or_init() -> AppConfig {
    if let Some(config) = load() {
        return config;
    }
    let config = AppConfig::fresh();
    if let Err(e) = save(&config) {
        tracing::warn!("could not persist initial config at {}: {e}", config_path().display());
    }
    config
}

fn generate_secret() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
