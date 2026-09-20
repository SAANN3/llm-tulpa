use std::path::PathBuf;
use std::sync::OnceLock;

use rand::RngCore;
use serde::{Deserialize, Serialize};

/// Postgres connection details entered during the setup wizard and persisted to settings.json
#[derive(Clone, Serialize, Deserialize)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String,
}

// Hand-written so a stray `{:?}` (a log line, a panic message) can never print the password.
impl std::fmt::Debug for DbConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("name", &self.name)
            .field("user", &self.user)
            .field("password", &"<hidden>")
            .finish()
    }
}

/// Percent-encodes everything outside RFC 3986's unreserved set, so a user name or password
/// containing `@`, `/`, `:` or `%` can't break out of its slot in the connection URL.
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

impl DbConfig {
    /// `postgres://user:password@host:port`, without a database name — the caller appends
    /// `/<db>`, since bootstrap connects to `/postgres` first to create the target database.
    pub fn base_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}",
            url_encode(&self.user),
            url_encode(&self.password),
            self.host,
            self.port
        )
    }
}

/// Where to reach Ollama and how much context to budget
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Where to reach Ollama.
    #[serde(default = "default_ollama_url")]
    pub url: String,
    /// Must match Ollama's own context window (`OLLAMA_CONTEXT_LENGTH` in `llm/`'s env) —
    /// drives the `num_predict` cap and the history-compaction thresholds.
    #[serde(default = "default_context_length")]
    pub context_length: u64,
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_context_length() -> u64 {
    32768
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            url: default_ollama_url(),
            context_length: default_context_length(),
        }
    }
}

/// Everything the backend is configured with, read from `settings.json` at startup. The
/// database block and the JWT secret are written by the app itself (first run and the setup
/// wizard); the rest is edited by hand — every field has a default, so a file that lists only
/// what differs is fine. `settings.example.json` in the repo lists them all.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub database: Option<DbConfig>,
    /// Generated on first run. Left out of a hand-written file, it's generated and saved on
    /// load (see `load_or_init`).
    #[serde(default)]
    pub jwt_secret: String,
    /// What the HTTP server binds to. Loopback-only by default; set `0.0.0.0:3000` to be
    /// reachable from other machines.
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default)]
    pub ollama: OllamaConfig,
    /// How many of a chat's most recent messages get pulled into a single turn.
    #[serde(default = "default_agent_history_len")]
    pub agent_history_len: u64,
    /// Where uploaded files are stored. Defaults to `~/.llm-tulpa/files`.
    #[serde(default)]
    pub files_dir: Option<PathBuf>,
    /// Where background jobs (`os.start_job`) write their log files. Defaults to `~/.llm-tulpa/jobs`.
    #[serde(default)]
    pub jobs_dir: Option<PathBuf>,
    /// How many days a finished job's log is kept; the sweep runs once at startup. `0` keeps
    /// every log.
    #[serde(default = "default_job_log_retention_days")]
    pub job_log_retention_days: u64,
    /// The SearXNG instance `web.search_query` calls — the rate-limiting sidecar in front of
    /// it (see the repo's `searxng/`), not SearXNG's own port.
    #[serde(default = "default_searxng_url")]
    pub searxng_url: String,
    /// Where the host's real root filesystem is mounted, when running in a container —
    /// `os.get_disk_space` reads it to report the host's disk rather than the container's.
    #[serde(default)]
    pub host_root: Option<String>,
    /// The folder holding local `.gguf` model files — the same one `llm/`'s `MODEL_DIR`
    /// points Ollama at. Unset disables importing local model files.
    #[serde(default)]
    pub model_dir: Option<PathBuf>,
}

fn default_bind_addr() -> String {
    "127.0.0.1:3000".to_string()
}

fn default_agent_history_len() -> u64 {
    200
}

fn default_job_log_retention_days() -> u64 {
    7
}

fn default_searxng_url() -> String {
    "http://localhost:8090".to_string()
}

impl AppConfig {
    fn fresh() -> Self {
        Self {
            database: None,
            jwt_secret: generate_secret(),
            bind_addr: default_bind_addr(),
            ollama: OllamaConfig::default(),
            agent_history_len: default_agent_history_len(),
            files_dir: None,
            jobs_dir: None,
            job_log_retention_days: default_job_log_retention_days(),
            searxng_url: default_searxng_url(),
            host_root: None,
            model_dir: None,
        }
    }

    /// `files_dir` if set, else `~/.llm-tulpa/files`. Panics without a home directory, since
    /// there's then nowhere sensible to put uploads — set `files_dir` explicitly.
    pub fn resolved_files_dir(&self) -> PathBuf {
        self.files_dir.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| panic!("could not determine home directory; set `files_dir` in settings.json"))
                .join(".llm-tulpa/files")
        })
    }

    /// `jobs_dir` if set, else `~/.llm-tulpa/jobs`. Panics without a home directory, like
    /// `resolved_files_dir`.
    pub fn resolved_jobs_dir(&self) -> PathBuf {
        self.jobs_dir.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| panic!("could not determine home directory; set `jobs_dir` in settings.json"))
                .join(".llm-tulpa/jobs")
        })
    }
}

/// The two settings a tool needs at call time. Tools have no handle on `AppState`, so
/// `install` copies these out once at startup into a process-wide read-only cell.
struct ToolSettings {
    searxng_url: String,
    host_root: Option<String>,
}

static TOOL_SETTINGS: OnceLock<ToolSettings> = OnceLock::new();

/// Publishes the config values tools read. Called once, from `main`, before serving.
pub fn install_tool_settings(config: &AppConfig) {
    let _ = TOOL_SETTINGS.set(ToolSettings {
        searxng_url: config.searxng_url.clone(),
        host_root: config.host_root.clone(),
    });
}

/// The SearXNG base URL, or the default when `install_tool_settings` hasn't run (unit tests).
pub fn searxng_url() -> String {
    TOOL_SETTINGS
        .get()
        .map(|s| s.searxng_url.clone())
        .unwrap_or_else(default_searxng_url)
}

/// Where the host's root filesystem is mounted, if configured.
pub fn host_root() -> Option<String> {
    TOOL_SETTINGS.get().and_then(|s| s.host_root.clone())
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

/// Loads the existing config, or creates+persists a fresh one on first run. A file that
/// exists but doesn't parse is a hard stop, not a reason to start over: silently generating
/// a fresh `jwt_secret` and forgetting the database connection would log everyone out and
/// send the wizard off to overwrite what the user just typo'd.
pub fn load_or_init() -> AppConfig {
    let path = config_path();
    if path.exists() {
        let mut config = load().unwrap_or_else(|| {
            panic!("{} exists but could not be parsed (see the error above) — fix or remove it", path.display())
        });
        // A hand-written file (copied from `settings.example.json`) has no secret yet.
        if config.jwt_secret.is_empty() {
            config.jwt_secret = generate_secret();
            if let Err(e) = save(&config) {
                tracing::warn!("could not save the generated JWT secret to {}: {e}", path.display());
            }
        }
        return config;
    }
    let config = AppConfig::fresh();
    if let Err(e) = save(&config) {
        tracing::warn!("could not persist initial config at {}: {e}", path.display());
    }
    config
}

fn generate_secret() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
