use std::path::PathBuf;
use std::sync::Arc;

use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};

use crate::cache::user_cache::UserCacheService;
use crate::facade::{agent::Agent, prompt::PromptFacade};
use crate::plugins::base::PluginBuilder;
use crate::plugins::messaging::builder::MessagingProviderBuilder;
use crate::plugins::messaging::discord::DiscordProvider;
use crate::plugins::messaging::telegram::TelegramProvider;
use crate::plugins::messaging::vk::VkProvider;
use crate::plugins::registry::PluginRegistry;
use crate::services::{
    chat_store::ChatStore, file_store::FileStore, llm::OllamaService, migrate::run_migrations,
    model_store::ModelStore, permission_store::PermissionStore,
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
    pub agent: Agent,
    pub prompt: PromptFacade,
    pub user_cache: Arc<UserCacheService>,
    pub plugin_registry: Arc<PluginRegistry>,
}

impl AppServices {
    /// Whether the database still answers — a cheap `SELECT 1`. The setup status uses this
    /// so a database that went away after startup reads as "not configured" and sends the
    /// frontend back to the wizard, rather than leaving every request to fail on its own.
    pub async fn ping(&self) -> bool {
        self.db.execute_unprepared("SELECT 1").await.is_ok()
    }
}

/// Connects to Postgres (creating the target database if missing), runs the centralized
/// migration, and builds every DB-backed service sharing that one connection. Returns an
/// error string (rather than panicking) so the setup wizard can report a bad connection
/// back to the user instead of crashing the process.
pub async fn bootstrap(
    db_base_url: &str,
    db_name: &str,
    files_dir: PathBuf,
    ollama: Arc<OllamaService>,
    tools: Arc<ToolService>,
    agent_history_len: u64,
    ollama_context_length: u64,
) -> Result<AppServices, String> {
    let admin = Database::connect(format!("{db_base_url}/postgres"))
        .await
        .map_err(|e| format!("could not connect to postgres: {e}"))?;

    let exists = admin
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "SELECT 1 FROM pg_database WHERE datname = $1",
            [db_name.into()],
        ))
        .await
        .map_err(|e| format!("could not check for database '{db_name}': {e}"))?
        .is_some();

    if !exists {
        admin
            .execute_unprepared(&format!("CREATE DATABASE \"{db_name}\""))
            .await
            .map_err(|e| format!("could not create database '{db_name}': {e}"))?;
    }
    admin.close().await.ok();

    let db = Database::connect(format!("{db_base_url}/{db_name}"))
        .await
        .map_err(|e| format!("could not connect to database '{db_name}': {e}"))?;

    run_migrations(&db).await.map_err(|e| format!("migration failed: {e}"))?;

    let user_store = Arc::new(UserStore::new(db.clone()));
    let model_store = Arc::new(ModelStore::new(db.clone()));
    let chat_store = Arc::new(ChatStore::new(db.clone(), model_store.clone()));
    let settings_store = Arc::new(SettingsStore::new(db.clone(), model_store.clone()));
    let permission_store = Arc::new(PermissionStore::new(db.clone()));
    let file_store = Arc::new(FileStore::new(db.clone(), files_dir).await);
    let plugin_settings_store = Arc::new(PluginSettingsStore::new(db.clone()));

    let agent = Agent::new(
        ollama.clone(),
        chat_store.clone(),
        tools.clone(),
        file_store.clone(),
        permission_store.clone(),
        agent_history_len,
        ollama_context_length,
    );
    let prompt = PromptFacade::new(ollama.clone());
    let user_cache = UserCacheService::new(settings_store.clone(), prompt.clone());

    // A tool-less agent for plugin conversations — real persistence and replies with zero
    // tool-calling risk, sharing the same stores as the main agent.
    let plugin_agent = Arc::new(Agent::new(
        ollama.clone(),
        chat_store.clone(),
        Arc::new(ToolService::new(vec![])),
        file_store.clone(),
        permission_store.clone(),
        agent_history_len,
        ollama_context_length,
    ));

    let plugin_registry = Arc::new(PluginRegistry::new(plugin_settings_store));
    let plugin_builders: Vec<Arc<dyn PluginBuilder>> = vec![
        Arc::new(MessagingProviderBuilder::<TelegramProvider>::new(plugin_agent.clone(), chat_store.clone())),
        Arc::new(MessagingProviderBuilder::<DiscordProvider>::new(plugin_agent.clone(), chat_store.clone())),
        Arc::new(MessagingProviderBuilder::<VkProvider>::new(plugin_agent, chat_store.clone())),
    ];
    plugin_registry
        .register_many(plugin_builders)
        .await
        .map_err(|e| format!("registering plugin builders failed: {e:?}"))?;

    Ok(AppServices {
        db,
        user_store,
        chat_store,
        settings_store,
        model_store,
        permission_store,
        file_store,
        agent,
        prompt,
        user_cache,
        plugin_registry,
    })
}
