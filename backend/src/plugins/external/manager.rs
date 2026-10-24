//! The external plugins as the backend runs them: found on disk at start-up, loaded and unloaded while it runs, recorded in
//! the database with how each is doing, and switched off when the supervisor gives up on one. This is the one place that
//! knows how a started plugin becomes a registered one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::http::StatusCode;
use sea_orm::DatabaseConnection;
use serde::Serialize;
use tokio::sync::{Mutex, mpsc};
use utoipa::ToSchema;

use super::loader::{plugin_folders, start_plugin};
use super::agent::AgentAccess;
use super::installer::InstallTask;
use super::supervisor::{Event, Status, Supervisor};
use crate::plugins::registry::PluginRegistry;
use crate::plugins::schema;
use crate::services::error::ErrorService;
use crate::services::external_plugin_store::{ExternalPluginRecord, ExternalPluginStore, PluginState};

type Key = (String, String);

/// An external plugin as the Plugins page lists it
#[derive(Serialize, ToSchema)]
pub struct ExternalPluginView {
    pub name: String,
    pub subname: String,
    pub version: Option<String>,
    pub kind: Option<String>,
    /// Where it came from: `folder` for one put in the plugins folder by hand
    pub source_type: String,
    /// The address it was installed from, for one that was not put in the folder by hand
    pub source_url: Option<String>,
    /// The git branch or tag it came from
    pub source_ref: Option<String>,
    /// The checksum of the file it was installed from
    pub source_sha256: Option<String>,
    pub state: PluginState,
    /// Why it is not running, when it is not
    pub last_error: Option<String>,
    #[schema(value_type = String)]
    pub installed_at: chrono::DateTime<chrono::Utc>,
    #[schema(value_type = String)]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Its process, while it has one: whether it is running or being restarted, its pid, how often it was restarted and the end
    /// of what it wrote to its log
    pub process: Option<Status>,
}

#[derive(Debug)]
pub enum ManagerError {
    NotFound(String),
    /// The plugin is there but cannot be loaded or unloaded as asked
    Refused(String),
    Failed(String),
}

impl From<crate::services::external_plugin_store::ExternalPluginStoreError> for ManagerError {
    fn from(e: crate::services::external_plugin_store::ExternalPluginStoreError) -> Self {
        ManagerError::Failed(e.to_string())
    }
}

impl From<ManagerError> for ErrorService {
    fn from(e: ManagerError) -> Self {
        match e {
            ManagerError::NotFound(what) => ErrorService::new(StatusCode::NOT_FOUND, what),
            ManagerError::Refused(why) => ErrorService::new(StatusCode::BAD_REQUEST, why),
            ManagerError::Failed(why) => ErrorService::internal(why),
        }
    }
}

pub struct ExternalPlugins {
    pub(super) root: PathBuf,
    pub(super) plugins_dir: PathBuf,
    db: DatabaseConnection,
    agent: Arc<AgentAccess>,
    pub(super) registry: Arc<PluginRegistry>,
    pub(super) store: Arc<ExternalPluginStore>,
    events: mpsc::UnboundedSender<Event>,
    /// The supervisor of each plugin that is loaded, for its status
    pub(super) loaded: Mutex<HashMap<Key, Arc<Supervisor>>>,
    /// The installs that ran or are running, by their staging id, newest kept
    pub(super) tasks: std::sync::Mutex<Vec<InstallTask>>,
    /// Held for the length of an install or an uninstall: one at a time
    pub(super) installing: Arc<Mutex<()>>,
}

impl ExternalPlugins {
    /// `root` is the folder of the plugins (`<root>/<name>/<subname>`), `plugins_dir` the one their data folders are in.
    pub fn new(root: PathBuf, plugins_dir: PathBuf, db: DatabaseConnection, agent: Arc<AgentAccess>, registry: Arc<PluginRegistry>, store: Arc<ExternalPluginStore>) -> Arc<Self> {
        let (events, mut heard) = mpsc::unbounded_channel();
        // A plugin the supervisor gave up on is switched off and recorded as failing. The task holds the registry and the
        // store, not the manager, and ends when the last supervisor is gone.
        let (listening_registry, listening_store) = (registry.clone(), store.clone());
        tokio::spawn(async move {
            while let Some(event) = heard.recv().await {
                let result = match event {
                    Event::GaveUp { name, subname, why } => {
                        if let Err(e) = listening_registry.set_enabled(&name, &subname, false).await {
                            tracing::warn!("plugin {name}/{subname} could not be switched off after it crashed: {e}");
                        }
                        listening_store.set_state(&name, &subname, PluginState::Failing, Some(&why)).await
                    }
                    Event::Revived { name, subname } => listening_store.set_state(&name, &subname, PluginState::Ok, None).await,
                };
                if let Err(e) = result {
                    tracing::warn!("{e}");
                }
            }
        });
        Arc::new(Self { root, plugins_dir, db, agent, registry, store, events, loaded: Mutex::new(HashMap::new()), tasks: std::sync::Mutex::new(Vec::new()), installing: Arc::new(Mutex::new(())) })
    }

    /// Starts every plugin in the plugins folder. One that cannot start is recorded with the reason and the others carry on;
    /// a plugin the database remembers whose folder is gone is recorded as unavailable.
    pub async fn start_all(&self) {
        super::stage::prune(&self.root);
        let mut seen: Vec<Key> = Vec::new();
        for dir in plugin_folders(&self.root) {
            let (Some(subname), Some(name)) = (dir.file_name(), dir.parent().and_then(|p| p.file_name())) else {
                continue;
            };
            let (name, subname) = (name.to_string_lossy().to_string(), subname.to_string_lossy().to_string());
            seen.push((name.clone(), subname.clone()));
            if let Err(e) = self.load_dir(&name, &subname).await {
                tracing::error!("external plugin {name}/{subname} is not available: {}", e.message());
            }
        }
        match self.store.list().await {
            Ok(records) => {
                for record in records.into_iter().filter(|r| !seen.contains(&(r.name.clone(), r.subname.clone()))) {
                    if record.state != PluginState::Unavailable || record.last_error.as_deref() != Some(GONE) {
                        self.note(&record.name, &record.subname, None, None, PluginState::Unavailable, Some(GONE)).await;
                    }
                }
            }
            Err(e) => tracing::warn!("{e}"),
        }
    }

    /// Loads the plugin in `<root>/<name>/<subname>` while the backend runs, as if it had been there at start-up.
    pub async fn load(&self, name: &str, subname: &str) -> Result<ExternalPluginView, ManagerError> {
        schema::prefix(name, subname).map_err(|e| ManagerError::Refused(e.to_string()))?;
        if self.registry.contains(name, subname).await {
            return Err(ManagerError::Refused(format!("{name}/{subname} is already loaded, or is a built-in plugin; unload it first")));
        }
        self.load_dir(name, subname).await?;
        self.view(name, subname).await
    }

    /// Stops a plugin and takes it out of the registry. Its files, its settings, its tables and its data stay.
    pub async fn unload(&self, name: &str, subname: &str) -> Result<(), ManagerError> {
        let key = (name.to_string(), subname.to_string());
        if !self.loaded.lock().await.contains_key(&key) {
            return Err(ManagerError::NotFound(format!("no loaded external plugin {name}/{subname}")));
        }
        self.registry.unregister(name, subname).await.map_err(|e| ManagerError::Failed(e.to_string()))?;
        self.loaded.lock().await.remove(&key);
        self.note_state(name, subname, PluginState::Unloaded, None).await;
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<ExternalPluginView>, ErrorService> {
        let records = self.store.list().await?;
        let loaded = self.loaded.lock().await;
        Ok(records
            .into_iter()
            .map(|record| {
                let process = loaded.get(&(record.name.clone(), record.subname.clone())).map(|s| s.status());
                view_of(record, process)
            })
            .collect())
    }

    pub(super) async fn view(&self, name: &str, subname: &str) -> Result<ExternalPluginView, ManagerError> {
        let records = self.store.list().await.map_err(|e| ManagerError::Failed(e.to_string()))?;
        let record = records.into_iter().find(|r| r.name == name && r.subname == subname).ok_or_else(|| ManagerError::NotFound(format!("{name}/{subname}")))?;
        let process = self.loaded.lock().await.get(&(name.to_string(), subname.to_string())).map(|s| s.status());
        Ok(view_of(record, process))
    }

    /// Starts and registers the plugin of a folder, and records how it went
    pub(super) async fn load_dir(&self, name: &str, subname: &str) -> Result<(), ManagerError> {
        let dir = self.root.join(name).join(subname);
        if !dir.join("plugin.toml").is_file() {
            return Err(ManagerError::NotFound(format!("there is no plugin.toml in {}", dir.display())));
        }
        let builder = match start_plugin(&dir, &self.plugins_dir, &self.db, &self.agent, self.events.clone()).await {
            Ok(builder) => builder,
            Err(e) => {
                let version = e.manifest.as_ref().map(|m| m.version.clone());
                self.note(name, subname, version.as_deref(), e.manifest.as_ref().map(|m| m.kind.as_str()), PluginState::Unavailable, Some(&e.why)).await;
                return Err(ManagerError::Refused(e.why));
            }
        };
        let supervisor = builder.supervisor().clone();
        let (version, kind) = (builder.manifest().version.clone(), builder.manifest().kind.as_str());
        let builder = Arc::new(builder);
        if let Err(e) = self.registry.register_late(builder.clone()).await {
            // A tool name that is taken leaves the plugin registered but off; it is taken out again so that it can be loaded
            // anew (the process is ended by the registry, or here when it never got that far)
            let _ = self.registry.unregister(name, subname).await;
            let why = e.to_string();
            self.note(name, subname, Some(&version), Some(kind), PluginState::Unavailable, Some(&why)).await;
            return Err(ManagerError::Refused(why));
        }
        self.loaded.lock().await.insert((name.to_string(), subname.to_string()), supervisor);
        self.note(name, subname, Some(&version), Some(kind), PluginState::Ok, None).await;
        Ok(())
    }

    /// Records how a plugin went. A record that cannot be written is logged: it is for show, and the plugin is not worse off.
    async fn note(&self, name: &str, subname: &str, version: Option<&str>, kind: Option<&str>, state: PluginState, last_error: Option<&str>) {
        if let Err(e) = self.store.record(name, subname, version, kind, state, last_error).await {
            tracing::warn!("{e}");
        }
    }

    async fn note_state(&self, name: &str, subname: &str, state: PluginState, last_error: Option<&str>) {
        if let Err(e) = self.store.set_state(name, subname, state, last_error).await {
            tracing::warn!("{e}");
        }
    }
}

/// Said of a plugin the database remembers and the plugins folder no longer has
const GONE: &str = "its folder is gone from the plugins folder";

fn view_of(record: ExternalPluginRecord, process: Option<Status>) -> ExternalPluginView {
    ExternalPluginView {
        name: record.name,
        subname: record.subname,
        version: record.version,
        kind: record.kind,
        source_type: record.source_type,
        source_url: record.source_url,
        source_ref: record.source_ref,
        source_sha256: record.source_sha256,
        state: record.state,
        last_error: record.last_error,
        installed_at: record.installed_at,
        updated_at: record.updated_at,
        process,
    }
}

impl ManagerError {
    pub fn message(&self) -> String {
        match self {
            ManagerError::NotFound(m) | ManagerError::Refused(m) | ManagerError::Failed(m) => m.clone(),
        }
    }
}
