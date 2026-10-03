use std::path::PathBuf;
use std::sync::Arc;

use axum::http::StatusCode;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr, Statement};

use crate::cache::user_cache::UserCacheService;
use crate::facade::{agent::Agent, export::ExportFacade, launch::LaunchFacade, prompt::PromptFacade, stats::StatsFacade};
use crate::plugins::base::{PluginBuilder, PluginError};
use crate::plugins::coding::signatures::builder::SignaturesBuilder;
use crate::plugins::messaging::builder::MessagingProviderBuilder;
use crate::plugins::messaging::discord::DiscordProvider;
use crate::plugins::messaging::telegram::TelegramProvider;
use crate::plugins::messaging::vk::VkProvider;
use crate::plugins::registry::PluginRegistry;
use crate::services::{
    chat_store::ChatStore, error::ErrorService, event_bus::EventBus, file_store::FileStore,
    folder_store::FolderStore, job_store::JobStore, launch_store::LaunchStore,
    llama_runtime::LlamaRuntime, llm::LlmProviders, migrate::run_migrations, preset_store::PresetStore,
    migrate::adopt_legacy_data, model_store::ModelStore, permission_store::PermissionStore,
    plugin_settings_store::PluginSettingsStore, settings_store::SettingsStore, tools::ToolService,
    user_store::UserStore,
};

/// All DB-backed services, bundled so `AppState` can hold them behind one lock and swap
/// them in atomically once the database is configured. Cloneable — every field is an
/// `Arc` or a facade of `Arc`s — so a handler cheaply snapshots the current set out of
/// the lock without holding it across an `.await`.
#[derive(Clone)]
pub struct AppServices {
    db: DatabaseConnection,
    pub user_store: Arc<UserStore>,
    pub chat_store: Arc<ChatStore>,
    pub settings_store: Arc<SettingsStore>,
    pub model_store: Arc<ModelStore>,
    pub permission_store: Arc<PermissionStore>,
    pub file_store: Arc<FileStore>,
    pub folder_store: Arc<FolderStore>,
    pub job_store: Arc<JobStore>,
    pub launch_store: Arc<LaunchStore>,
    pub preset_store: Arc<PresetStore>,
    pub launches: Arc<LaunchFacade>,
    pub agent: Arc<Agent>,
    pub prompt: PromptFacade,
    pub stats: StatsFacade,
    pub export: ExportFacade,
    pub user_cache: Arc<UserCacheService>,
    pub plugin_registry: Arc<PluginRegistry>,
    /// The context window the agent runs under — Ollama's configured ceiling
    /// (`ollama.context_length`), the number `last_prompt_tokens` is budgeted against.
    /// Exposed so a client can show a chat's context usage as `used / max`.
    pub context_length: u64,
}

/// The model a single-user install ran — `llm/start.sh` registers the one model it serves under
/// this name. Its chats are bound to it when their data is adopted (see `adopt_legacy_data`).
const LEGACY_MODEL_NAME: &str = "local-llm";

impl AppServices {
    /// Hands whatever a previous single-user install left in the database (see
    /// `services::migrate`) to `owner_id`. A no-op when there's nothing to adopt, so it's safe
    /// to call after every owner creation.
    pub async fn adopt_legacy_data(&self, owner_id: i64) -> Result<(), ErrorService> {
        let adopted = adopt_legacy_data(&self.db, owner_id, LEGACY_MODEL_NAME).await.map_err(|e| {
            tracing::error!("adopting legacy data failed: {e}");
            ErrorService::internal("could not import the previous install's data")
        })?;

        if adopted {
            tracing::info!("adopted the previous single-user install's data for user {owner_id}");
            // The imported plugin settings weren't there when the registry was built.
            self.plugin_registry.reload_unconfigured().await;
        }
        Ok(())
    }

    /// Whether the database still answers — a cheap `SELECT 1`. The setup status uses this
    /// so a database that went away after startup reads as "not configured" and sends the
    /// frontend back to the wizard, rather than leaving every request to fail on its own.
    pub async fn ping(&self) -> bool {
        self.db.execute_unprepared("SELECT 1").await.is_ok()
    }

    /// Whether a release has changed the setup since the owner last went through it.
    pub async fn setup_update_available(&self) -> bool {
        crate::services::migrate::setup_revision(&self.db).await.is_ok_and(|rev| rev < crate::services::migrate::SETUP_REVISION)
    }

    pub async fn complete_setup(&self) -> Result<(), ErrorService> {
        crate::services::migrate::complete_setup(&self.db).await.map_err(|e| {
            tracing::error!("could not record the setup as complete: {e}");
            ErrorService::internal("database query failed")
        })
    }
}

/// Why `bootstrap` failed. The `Display` form carries the driver's own detail for the log;
/// what a *caller over HTTP* gets to see is `public_message`, which never echoes connection
/// details (host, user, driver text) back to an unauthenticated setup request.
#[derive(Debug)]
pub enum BootstrapError {
    /// The database name isn't a plain identifier — rejected before it gets anywhere near SQL.
    InvalidDatabaseName,
    Connect(DbErr),
    CreateDatabase(DbErr),
    Migration(DbErr),
    Plugins(PluginError),
}

impl BootstrapError {
    pub fn public_message(&self) -> &'static str {
        match self {
            Self::InvalidDatabaseName => "the database name may only contain letters, digits and underscores",
            Self::Connect(_) | Self::CreateDatabase(_) => "could not connect to the database with those details",
            Self::Migration(_) => "the database could not be migrated to the current schema",
            Self::Plugins(_) => "the plugins could not be started",
        }
    }
}

impl std::fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDatabaseName => write!(f, "invalid database name"),
            Self::Connect(e) => write!(f, "could not connect to postgres: {e}"),
            Self::CreateDatabase(e) => write!(f, "could not create the database: {e}"),
            Self::Migration(e) => write!(f, "migration failed: {e}"),
            Self::Plugins(e) => write!(f, "registering plugin builders failed: {e:?}"),
        }
    }
}

impl From<BootstrapError> for ErrorService {
    fn from(err: BootstrapError) -> Self {
        tracing::warn!("bootstrap failed: {err}");
        let status = match err {
            BootstrapError::Migration(_) | BootstrapError::Plugins(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        };
        ErrorService::new(status, err.public_message())
    }
}

/// Whether `name` is safe to interpolate as a quoted Postgres identifier. `CREATE DATABASE`
/// can't take a bind parameter, so the name has to be validated instead: letters, digits and
/// underscores, not starting with a digit, within Postgres's 63-byte identifier limit.
fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.len() <= 63
}

/// Connects to Postgres (creating the target database if missing), runs the centralized
/// migration, and builds every DB-backed service sharing that one connection. Returns an
/// error (rather than panicking) so the setup wizard can report a bad connection back to
/// the user instead of crashing the process.
pub async fn bootstrap(
    db_base_url: &str,
    db_name: &str,
    files_dir: PathBuf,
    jobs_dir: PathBuf,
    job_log_retention_days: u64,
    providers: LlmProviders,
    tools: Arc<ToolService>,
    events: Arc<EventBus>,
    agent_history_len: u64,
    ollama_context_length: u64,
    model_dir: Option<PathBuf>,
    runtime: Arc<LlamaRuntime>,
) -> Result<AppServices, BootstrapError> {
    if !is_plain_identifier(db_name) {
        return Err(BootstrapError::InvalidDatabaseName);
    }

    let admin = Database::connect(format!("{db_base_url}/postgres"))
        .await
        .map_err(BootstrapError::Connect)?;

    let exists = admin
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 FROM pg_database WHERE datname = $1",
            [db_name.into()],
        ))
        .await
        .map_err(BootstrapError::Connect)?
        .is_some();

    if !exists {
        admin
            .execute_unprepared(&format!("CREATE DATABASE \"{db_name}\""))
            .await
            .map_err(BootstrapError::CreateDatabase)?;
    }
    admin.close().await.ok();

    let db = Database::connect(format!("{db_base_url}/{db_name}"))
        .await
        .map_err(BootstrapError::Connect)?;

    run_migrations(&db).await.map_err(BootstrapError::Migration)?;

    let user_store = Arc::new(UserStore::new(db.clone()));
    let model_store = Arc::new(ModelStore::new(db.clone()));
    let folder_store = Arc::new(FolderStore::new(db.clone()));
    let launch_store = Arc::new(LaunchStore::new(db.clone()));
    let preset_store = Arc::new(PresetStore::new(db.clone()));
    let chat_store = Arc::new(ChatStore::new(
        db.clone(),
        model_store.clone(),
        user_store.clone(),
        folder_store.clone(),
        launch_store.clone(),
    ));
    let settings_store = Arc::new(SettingsStore::new(db.clone(), model_store.clone()));
    let permission_store = Arc::new(PermissionStore::new(db.clone()));
    let file_store = Arc::new(FileStore::new(db.clone(), files_dir).await);
    let job_store = Arc::new(JobStore::new(db.clone(), jobs_dir, job_log_retention_days, events.clone()).await);
    let plugin_settings_store = Arc::new(PluginSettingsStore::new(db.clone(), user_store.clone()));

    let launches = Arc::new(LaunchFacade::new(providers.clone(), runtime.clone(), launch_store.clone(), model_store.clone(), user_store.clone(), model_dir));

    let agent = Arc::new(Agent::new(
        providers.clone(),
        chat_store.clone(),
        tools.clone(),
        file_store.clone(),
        job_store.clone(),
        events.clone(),
        permission_store.clone(),
        settings_store.clone(),
        preset_store.clone(),
        launches.clone(),
        agent_history_len,
        ollama_context_length,
    ));
    agent.bind_subagent_runner();
    let prompt = PromptFacade::new(providers.clone());
    let stats = StatsFacade::new(
        chat_store.clone(),
        job_store.clone(),
        settings_store.clone(),
        providers.clone(),
        runtime.clone(),
        ollama_context_length,
    );
    let export = ExportFacade::new(chat_store.clone(), file_store.clone(), settings_store.clone());
    let user_cache = UserCacheService::new(settings_store.clone(), prompt.clone());

    // A tool-less agent for plugin conversations — real persistence and replies with zero
    // tool-calling risk, sharing the same stores as the main agent.
    let plugin_agent = Arc::new(Agent::new(
        providers.clone(),
        chat_store.clone(),
        Arc::new(ToolService::new(vec![])),
        file_store.clone(),
        job_store.clone(),
        events,
        permission_store.clone(),
        settings_store.clone(),
        preset_store.clone(),
        launches.clone(),
        agent_history_len,
        ollama_context_length,
    ));

    let plugin_registry = Arc::new(PluginRegistry::new(plugin_settings_store, tools.clone()));
    let plugin_builders: Vec<Arc<dyn PluginBuilder>> = vec![
        Arc::new(MessagingProviderBuilder::<TelegramProvider>::new(plugin_agent.clone(), chat_store.clone())),
        Arc::new(MessagingProviderBuilder::<DiscordProvider>::new(plugin_agent.clone(), chat_store.clone())),
        Arc::new(MessagingProviderBuilder::<VkProvider>::new(plugin_agent, chat_store.clone())),
        Arc::new(SignaturesBuilder),
    ];
    plugin_registry
        .register_many(plugin_builders)
        .await
        .map_err(BootstrapError::Plugins)?;

    let services = AppServices {
        db,
        user_store,
        chat_store,
        settings_store,
        model_store,
        permission_store,
        file_store,
        folder_store,
        job_store,
        launch_store,
        preset_store,
        launches,
        agent,
        prompt,
        stats,
        export,
        user_cache,
        plugin_registry,
        context_length: ollama_context_length,
    };

    // What a call that names no launch runs when nothing is loaded, and what the server loads at
    // boot when `llama_cpp.autostart` is on. Best effort: no owner or model yet is the normal case
    // before setup.
    match services.launches.default_request().await {
        Ok(Some(request)) => runtime.remember_default(request),
        Ok(None) => {}
        Err(e) => tracing::warn!("could not work out the default model launch: {}", e.message.unwrap_or_default()),
    }

    // A previous adoption that didn't finish (the owner exists, the legacy data is still there)
    // is retried here rather than waiting for a first-run request that will never come again.
    if let Ok(Some(owner_id)) = services.user_store.owner_id().await {
        if let Err(e) = services.adopt_legacy_data(owner_id).await {
            tracing::warn!("legacy data adoption at startup failed: {}", e.message.unwrap_or_default());
        }
    }

    Ok(services)
}
