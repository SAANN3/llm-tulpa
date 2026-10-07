use std::sync::Arc;

use axum::http::StatusCode;
use tokio::sync::{Mutex, RwLock};

use crate::config::{self, AppConfig, DbConfig};
use crate::services::{
    auth::AuthService,
    bootstrap::{bootstrap, AppServices, BootstrapError},
    error::ErrorService,
    event_bus::EventBus,
    hf_library::HfLibrary,
    llama_install::LlamaInstaller,
    llama_runtime::LlamaRuntime,
    model_folder::ModelFolder,
    llm::{LlmProviders, OllamaService},
    model_library::ModelLibrary,
    tools::ToolService,
};

pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub auth: AuthService,
    /// Everything that runs a model goes through here; Ollama's own library (catalog, pull,
    /// import) is `library`, which holds the Ollama client it needs.
    pub providers: LlmProviders,
    /// The Ollama client itself, for changing where it points (providers hold it as a trait object)
    pub ollama: Arc<OllamaService>,
    pub tools: Arc<ToolService>,
    /// The llama-server the backend runs itself: which profile it has loaded, its log, who holds it.
    pub runtime: Arc<LlamaRuntime>,
    /// Cancelled when the backend is asked to stop: long-lived responses (the event stream) end
    /// on it, since the server's graceful shutdown waits for every open connection.
    pub shutdown: tokio_util::sync::CancellationToken,
    /// Downloads llama.cpp (or records the user's own build) for `runtime` to run
    pub installer: Arc<LlamaInstaller>,
    /// Where the model files live; the owner can change it while the backend runs
    pub model_folder: Arc<ModelFolder>,
    /// What the backend broadcasts to connected frontends (`GET /api/live`). Needs no
    /// database, so it lives here rather than in `AppServices`; the job store and the agent
    /// get a handle to it when the services are built.
    pub events: Arc<EventBus>,
    /// The public model catalog, local model files, and the pull/import tasks — none of it
    /// needs the database, so it works before setup completes.
    pub library: Arc<ModelLibrary>,
    /// Hugging Face search and downloads into the model folder
    pub hf: Arc<HfLibrary>,
    pub services: Arc<RwLock<Option<AppServices>>>,
    /// Serializes anything that builds `AppServices`. Building them starts the enabled
    /// plugins' background loops, so two builds racing would leave two copies of every bot
    /// polling the same account.
    connect_lock: Mutex<()>,
}

/// Why `AppState::configure` refused or failed.
pub enum ConfigureError {
    /// A database was configured before — changing it is a deliberate edit of `settings.json`,
    /// not something an unauthenticated setup request may do.
    AlreadyConfigured,
    Bootstrap(BootstrapError),
    Save(std::io::Error),
}

impl From<ConfigureError> for ErrorService {
    fn from(err: ConfigureError) -> Self {
        match err {
            ConfigureError::AlreadyConfigured => ErrorService::new(
                StatusCode::FORBIDDEN,
                "a database is already configured; edit data/settings.json to change it",
            ),
            ConfigureError::Bootstrap(e) => e.into(),
            ConfigureError::Save(e) => {
                tracing::error!("could not save config: {e}");
                ErrorService::internal("could not save the configuration")
            }
        }
    }
}

impl AppState {
    pub fn new(
        config: AppConfig,
        ollama: Arc<OllamaService>,
        providers: LlmProviders,
        tools: Arc<ToolService>,
        events: Arc<EventBus>,
        runtime: Arc<LlamaRuntime>,
        model_folder: Arc<ModelFolder>,
    ) -> Self {
        let installer = LlamaInstaller::new(runtime.clone(), config.llama_cpp.resolved_dir());
        Self {
            auth: AuthService::new(&config.jwt_secret),
            library: Arc::new(ModelLibrary::new(ollama.clone(), model_folder.clone())),
            ollama,
            hf: Arc::new(HfLibrary::new(model_folder.clone())),
            config: Arc::new(RwLock::new(config)),
            providers,
            tools,
            events,
            installer,
            model_folder,
            shutdown: tokio_util::sync::CancellationToken::new(),
            runtime,
            services: Arc::new(RwLock::new(None)),
            connect_lock: Mutex::new(()),
        }
    }

    pub async fn services(&self) -> Result<AppServices, ErrorService> {
        self.services.read().await.clone().ok_or_else(|| {
            ErrorService::new(StatusCode::SERVICE_UNAVAILABLE, "the backend is not configured yet")
        })
    }

    /// Refuses a model that isn't really there, before it gets registered and bound to a chat or
    /// a user's settings: `ModelStore::ensure` would happily register any string, and every
    /// later turn would then fail against a model Ollama has never heard of. Installed models
    /// (per Ollama, live) pass; if Ollama can't be asked, only one already registered does.
    pub async fn require_installed_model(&self, services: &AppServices, provider: &str, name: &str) -> Result<(), ErrorService> {
        // A llama-server serves only the model it has loaded, so what it lists says nothing about
        // which model files exist: for the one the backend runs itself, registered is installed.
        let installed = match self.providers.get(provider) {
            Ok(_) if provider == "llama-cpp" => None,
            Ok(provider) => provider.list_local_models().await.ok().map(|models| models.iter().any(|m| m.name == name)),
            Err(_) => None,
        };

        let known = match installed {
            Some(installed) => installed,
            None => services.model_store.is_registered(provider, name).await?,
        };
        if known {
            Ok(())
        } else {
            Err(ErrorService::new(StatusCode::BAD_REQUEST, format!("model '{name}' isn't installed")))
        }
    }

    async fn build_services(&self, db: &DbConfig) -> Result<AppServices, BootstrapError> {
        let config = self.config.read().await.clone();
        bootstrap(
            &db.base_url(),
            &db.name,
            config.resolved_files_dir(),
            config.resolved_jobs_dir(),
            config.job_log_retention_days,
            self.providers.clone(),
            self.tools.clone(),
            self.events.clone(),
            config.ollama.context_length,
            self.model_folder.clone(),
            self.runtime.clone(),
        )
        .await
    }

    /// Connects using the database already in `settings.json`, if the services aren't up yet
    /// — startup does this once, and the setup status poll retries it, so a Postgres that
    /// wasn't ready when the backend booted doesn't leave it stuck in setup mode until a
    /// restart. `Ok(false)` means nothing is configured to connect to.
    pub async fn connect_configured(&self) -> Result<bool, BootstrapError> {
        let _guard = self.connect_lock.lock().await;
        if self.services.read().await.is_some() {
            return Ok(true);
        }
        let Some(db) = self.config.read().await.database.clone() else {
            return Ok(false);
        };
        let services = self.build_services(&db).await?;
        *self.services.write().await = Some(services);
        Ok(true)
    }

    /// The first-run wizard's database step: connects with the given details and, only on
    /// success, persists them and swaps the live services in. Refused once a database has
    /// been configured (see `ConfigureError::AlreadyConfigured`).
    pub async fn configure(&self, db: DbConfig) -> Result<(), ConfigureError> {
        let _guard = self.connect_lock.lock().await;
        if self.config.read().await.database.is_some() || self.services.read().await.is_some() {
            return Err(ConfigureError::AlreadyConfigured);
        }

        let services = self.build_services(&db).await.map_err(ConfigureError::Bootstrap)?;

        {
            let mut cfg = self.config.write().await;
            cfg.database = Some(db);
            config::save(&cfg).map_err(ConfigureError::Save)?;
        }
        *self.services.write().await = Some(services);
        Ok(())
    }
}
