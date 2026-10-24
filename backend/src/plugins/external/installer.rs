//! Installing, updating, uninstalling and exporting external plugins. An install is two steps the owner takes: the plugin is
//! staged (a `.tulpa` file or a git address, unpacked and described) and then, once they have seen what is in it, installed
//! as a background task that builds it, tries it, puts it in place and loads it. An update swaps in the new version only
//! after it has built and started, and puts the old one back if the new one will not load.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use utoipa::ToSchema;

use super::export::zip_folder;
use super::install::{run_build, trial_start, verify_prebuilt};
use super::loader::{program_exists, run_check};
use super::manager::{ExternalPlugins, ManagerError};
use super::manifest::{Kind, Manifest, version_numbers};
use super::stage::{self, StagedFile, StagedSource, staging_dir, tree_dir};
use crate::plugins::schema;
use crate::services::external_plugin_store::PluginSource;

/// The lines of one install's log that are kept
const MAX_LOG_LINES: usize = 3000;
/// How many finished installs are remembered
const MAX_TASKS: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StageAction {
    /// Nothing of that name is installed
    Install,
    /// A plugin of that name is installed; this replaces it
    Update,
}

/// A plugin that has been unpacked and looked at, waiting for the owner to say whether to install it
#[derive(Serialize, ToSchema)]
pub struct StagedPlugin {
    pub id: String,
    pub source: StagedSource,
    pub name: String,
    pub subname: String,
    pub version: String,
    pub kind: String,
    pub summary: String,
    pub help_message: String,
    /// The text of its `plugin.toml`, which says what will run and what it asks for
    pub plugin_toml: String,
    pub readme: Option<String>,
    /// The command that builds it, for a plugin of kind source
    pub build: Option<String>,
    /// The command that starts it on this machine
    pub run: String,
    pub check: Option<String>,
    /// The names of the settings it asks the owner for
    pub settings: Vec<String>,
    pub has_tab: bool,
    pub has_database: bool,
    pub files: Vec<StagedFile>,
    pub files_truncated: bool,
    pub total_bytes: u64,
    pub action: StageAction,
    /// The version installed now, when this would replace one
    pub installed_version: Option<String>,
    /// Why it cannot be installed; empty when it can
    pub problems: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Running,
    Done,
    Failed,
}

/// One install, from the owner's confirmation to its end
#[derive(Clone, Serialize, ToSchema)]
pub struct InstallTask {
    /// The id of the staged plugin it installs
    pub id: String,
    pub name: String,
    pub subname: String,
    pub state: TaskState,
    /// What it is doing now
    pub step: String,
    /// Everything it said, build output included, the last lines
    pub log: Vec<String>,
    pub error: Option<String>,
}

impl ExternalPlugins {
    /// Unpacks an uploaded `.tulpa` file and describes it.
    pub async fn stage_file(&self, bytes: Vec<u8>) -> Result<StagedPlugin, ManagerError> {
        stage::prune(&self.root);
        let (id, dir) = stage::new_staging(&self.root).map_err(ManagerError::Failed)?;
        let tree = tree_dir(&dir);
        let sha256 = stage::sha256_of_bytes(&bytes);
        let unpacked = tokio::task::spawn_blocking(move || stage::unpack_zip(&bytes, &tree)).await.map_err(|e| ManagerError::Failed(e.to_string()))?;
        self.finish_staging(&id, &dir, unpacked, StagedSource { source_type: "file".to_string(), sha256: Some(sha256), ..Default::default() }).await
    }

    /// Fetches a git repository and describes it.
    pub async fn stage_git(&self, url: &str, git_ref: Option<&str>) -> Result<StagedPlugin, ManagerError> {
        if !stage::valid_git_url(url) {
            return Err(ManagerError::Refused("that is not an address git can fetch from: use https://..., ssh://... or user@host:path".to_string()));
        }
        if let Some(git_ref) = git_ref {
            if !stage::valid_git_ref(git_ref) {
                return Err(ManagerError::Refused("the branch or tag may hold letters, digits and . _ / - only".to_string()));
            }
        }
        stage::prune(&self.root);
        let (id, dir) = stage::new_staging(&self.root).map_err(ManagerError::Failed)?;
        // Git wants to make the folder itself
        let tree = tree_dir(&dir);
        let _ = std::fs::remove_dir(&tree);
        let fetched = stage::git_clone(url, git_ref, &tree).await;
        let (commit, unpacked) = match fetched {
            Ok(commit) => (Some(commit), Ok(())),
            Err(e) => (None, Err(e)),
        };
        let source = StagedSource { source_type: "git".to_string(), url: Some(url.to_string()), git_ref: git_ref.map(str::to_string), commit, ..Default::default() };
        self.finish_staging(&id, &dir, unpacked, source).await
    }

    /// What staging a plugin came to: described, or taken away again with the reason
    async fn finish_staging(&self, id: &str, dir: &Path, unpacked: Result<(), String>, source: StagedSource) -> Result<StagedPlugin, ManagerError> {
        let described = match unpacked {
            Ok(()) => match stage::save_source(dir, &source) {
                Ok(()) => self.describe(id).await,
                Err(e) => Err(ManagerError::Failed(e)),
            },
            Err(why) => Err(ManagerError::Refused(why)),
        };
        if described.is_err() {
            let _ = tokio::fs::remove_dir_all(dir).await;
        }
        described
    }

    /// A staged plugin as the owner sees it before deciding
    pub async fn describe(&self, id: &str) -> Result<StagedPlugin, ManagerError> {
        let dir = staging_dir(&self.root, id).filter(|d| d.is_dir()).ok_or_else(|| ManagerError::NotFound("no such staged plugin; it may have expired".to_string()))?;
        let tree = tree_dir(&dir);
        let (manifest, plugin_toml) = stage::manifest_of(&tree).map_err(ManagerError::Refused)?;
        let survey_tree = tree.clone();
        let (files, total_bytes) = tokio::task::spawn_blocking(move || stage::survey(&survey_tree))
            .await
            .map_err(|e| ManagerError::Failed(e.to_string()))?
            .map_err(ManagerError::Refused)?;
        let (files, files_truncated) = stage::listed(files);

        let installed = self.store.list().await?.into_iter().find(|r| r.name == manifest.name && r.subname == manifest.subname);
        let loaded = self.loaded.lock().await.contains_key(&(manifest.name.clone(), manifest.subname.clone()));
        let mut problems = Vec::new();
        if installed.is_none() && self.registry.contains(&manifest.name, &manifest.subname).await {
            problems.push(format!("{}/{} is the name of a built-in plugin", manifest.name, manifest.subname));
        }
        let installed_version = installed.as_ref().and_then(|r| r.version.clone());
        // An installed plugin that is running is replaced only by a higher version; one that could not be loaded may be replaced by any
        if let (true, Some(old)) = (loaded, &installed_version) {
            if version_numbers(&manifest.version) <= version_numbers(old) {
                problems.push(format!("version {old} is installed and this is {}: only a higher version updates it; uninstall it first to install this one", manifest.version));
            }
        }
        if self.tasks.lock().unwrap().iter().any(|t| t.state == TaskState::Running && t.name == manifest.name && t.subname == manifest.subname) {
            problems.push("it is being installed right now".to_string());
        }
        Ok(StagedPlugin {
            id: id.to_string(),
            source: stage::load_source(&dir),
            name: manifest.name.clone(),
            subname: manifest.subname.clone(),
            version: manifest.version.clone(),
            kind: manifest.kind.as_str().to_string(),
            summary: manifest.summary.clone(),
            help_message: manifest.help_message.clone(),
            plugin_toml,
            readme: stage::readme(&tree),
            build: manifest.build.clone(),
            run: manifest.run_command(),
            check: manifest.check.clone(),
            settings: manifest.settings.iter().map(|s| s.name.clone()).collect(),
            has_tab: manifest.tab.is_some(),
            has_database: manifest.database.is_some(),
            files,
            files_truncated,
            total_bytes,
            action: if installed.is_some() { StageAction::Update } else { StageAction::Install },
            installed_version,
            problems,
        })
    }

    /// One file of a staged plugin, as text
    pub async fn staged_file(&self, id: &str, path: &str) -> Result<String, ManagerError> {
        let dir = staging_dir(&self.root, id).filter(|d| d.is_dir()).ok_or_else(|| ManagerError::NotFound("no such staged plugin; it may have expired".to_string()))?;
        let (tree, path) = (tree_dir(&dir), path.to_string());
        tokio::task::spawn_blocking(move || stage::read_staged_file(&tree, &path))
            .await
            .map_err(|e| ManagerError::Failed(e.to_string()))?
            .map_err(ManagerError::Refused)
    }

    /// Throws a staged plugin away
    pub async fn discard(&self, id: &str) -> Result<(), ManagerError> {
        let dir = staging_dir(&self.root, id).filter(|d| d.is_dir()).ok_or_else(|| ManagerError::NotFound("no such staged plugin".to_string()))?;
        if self.tasks.lock().unwrap().iter().any(|t| t.id == id && t.state == TaskState::Running) {
            return Err(ManagerError::Refused("it is being installed right now".to_string()));
        }
        tokio::fs::remove_dir_all(&dir).await.map_err(|e| ManagerError::Failed(format!("cannot remove it: {e}")))
    }

    /// Starts installing a staged plugin in the background; the task is followed with `task`.
    pub async fn start_install(self: &Arc<Self>, id: &str) -> Result<InstallTask, ManagerError> {
        let staged = self.describe(id).await?;
        if !staged.problems.is_empty() {
            return Err(ManagerError::Refused(staged.problems.join("; ")));
        }
        let guard = self.installing.clone().try_lock_owned().map_err(|_| ManagerError::Refused("another install or uninstall is running; wait for it to end".to_string()))?;
        let task = InstallTask { id: id.to_string(), name: staged.name.clone(), subname: staged.subname.clone(), state: TaskState::Running, step: "starting".to_string(), log: vec![], error: None };
        {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.retain(|t| t.id != id);
            tasks.push(task.clone());
            while tasks.len() > MAX_TASKS {
                tasks.remove(0);
            }
        }
        let this = self.clone();
        let id = id.to_string();
        tokio::spawn(async move {
            let _guard = guard;
            let result = this.install(&id).await;
            this.update_task(&id, |t| match &result {
                Ok(()) => {
                    t.state = TaskState::Done;
                    t.step = "done".to_string();
                }
                Err(why) => {
                    t.state = TaskState::Failed;
                    t.error = Some(why.clone());
                }
            });
            if let Err(why) = result {
                tracing::warn!("installing a plugin failed: {why}");
                // What was staged is useless after a failure the owner cannot fix from here; the log stays in the task
                if let Some(dir) = staging_dir(&this.root, &id) {
                    let _ = tokio::fs::remove_dir_all(dir).await;
                }
            }
        });
        Ok(task)
    }

    pub fn task(&self, id: &str) -> Option<InstallTask> {
        self.tasks.lock().unwrap().iter().find(|t| t.id == id).cloned()
    }

    fn update_task(&self, id: &str, change: impl FnOnce(&mut InstallTask)) {
        if let Some(task) = self.tasks.lock().unwrap().iter_mut().find(|t| t.id == id) {
            change(task);
        }
    }

    fn say(&self, id: &str, line: impl Into<String>) {
        let line = line.into();
        self.update_task(id, |t| {
            t.log.push(line);
            if t.log.len() > MAX_LOG_LINES {
                t.log.remove(0);
            }
        });
    }

    fn step(&self, id: &str, step: &str) {
        self.update_task(id, |t| t.step = step.to_string());
        self.say(id, format!("== {step}"));
    }

    /// The steps of an install, in order; an `Err` is the reason it failed and nothing is left half-done.
    async fn install(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let dir = staging_dir(&self.root, id).ok_or("no such staged plugin")?;
        let tree = tree_dir(&dir);
        let (manifest, _) = stage::manifest_of(&tree)?;
        let (name, subname) = (manifest.name.clone(), manifest.subname.clone());
        let data_dir = self.plugins_dir.join(&name).join(&subname);
        let source = stage::load_source(&dir);
        let replaces_running = self.loaded.lock().await.contains_key(&(name.clone(), subname.clone()));

        self.step(id, "checking what it needs");
        let check_ok = match &manifest.check {
            Some(check) => match run_check(check, &tree, &data_dir).await {
                Ok(()) => {
                    self.say(id, "the check passed");
                    true
                }
                // A new plugin may be installed and waits for what it needs; one that is running is not replaced by a version that
                // would not start
                Err(why) => {
                    self.say(id, format!("the check failed: {why}"));
                    if replaces_running {
                        return Err(format!("the new version needs something that is missing, so the installed version was left as it is: {why}"));
                    }
                    self.say(id, "the plugin will be installed, but cannot be switched on until what it needs is there");
                    false
                }
            },
            None => true,
        };

        match manifest.kind {
            Kind::Source => match &manifest.build {
                Some(build) => {
                    self.step(id, "building");
                    let this = self.clone();
                    let task_id = id.to_string();
                    run_build(build, &tree, &data_dir, move |line| this.say(&task_id, line)).await?;
                }
                None => self.say(id, "there is nothing to build"),
            },
            Kind::Prebuilt => {
                self.step(id, "checking the binary");
                let (check_manifest, check_tree) = (manifest.clone(), tree.clone());
                tokio::task::spawn_blocking(move || verify_prebuilt(&check_manifest, &check_tree)).await.map_err(|e| e.to_string())??;
            }
        }

        if check_ok {
            self.step(id, "starting it once to see that it runs");
            program_exists(&manifest, &tree, &data_dir)?;
            trial_start(&manifest, &tree, &data_dir).await?;
        } else {
            self.say(id, "it was not started: its check failed");
        }

        self.step(id, "putting it in place");
        self.put_in_place(id, &dir, &manifest, &source, check_ok).await?;
        let _ = tokio::fs::remove_dir_all(&dir).await;
        self.say(id, format!("{name}/{subname} {} is installed", manifest.version));
        Ok(())
    }

    /// Moves the staged tree to where plugins live and loads it. A plugin of that name that was installed is unloaded first and
    /// put back, running again, if the new one will not load.
    async fn put_in_place(&self, id: &str, staging: &Path, manifest: &Manifest, source: &StagedSource, check_ok: bool) -> Result<(), String> {
        let (name, subname) = (manifest.name.as_str(), manifest.subname.as_str());
        let tree = tree_dir(staging);
        let final_dir = self.root.join(name).join(subname);
        let old_dir = staging.join("old");
        let was_loaded = self.loaded.lock().await.contains_key(&(name.to_string(), subname.to_string()));
        let was_enabled = self.registry.is_enabled(name, subname).await;
        let had_folder = final_dir.is_dir();

        if was_loaded {
            self.say(id, "stopping the version that is installed");
            self.unload(name, subname).await.map_err(|e| e.message())?;
        }
        tokio::fs::create_dir_all(self.root.join(name)).await.map_err(|e| format!("cannot make the folder of {name}: {e}"))?;
        if had_folder {
            tokio::fs::rename(&final_dir, &old_dir).await.map_err(|e| format!("cannot move the installed version aside: {e}"))?;
        }
        if let Err(e) = tokio::fs::rename(&tree, &final_dir).await {
            if had_folder {
                let _ = tokio::fs::rename(&old_dir, &final_dir).await;
                if was_loaded {
                    self.bring_back(name, subname, was_enabled).await;
                }
            }
            return Err(format!("cannot put it in place: {e}"));
        }

        self.step(id, "loading it");
        let loaded = self.load_dir(name, subname).await;
        // A plugin whose check failed stays installed, recorded as unavailable with what is missing; `load` brings it up once that is there
        let waits_for_its_check = !check_ok && loaded.is_err();
        if waits_for_its_check {
            self.say(id, "it is installed but not loaded: what it needs is missing");
        }
        if let (Err(e), false) = (loaded, waits_for_its_check) {
            let why = e.message();
            let _ = self.registry.unregister(name, subname).await;
            let _ = tokio::fs::remove_dir_all(&final_dir).await;
            if had_folder {
                self.say(id, format!("the new version could not be loaded ({why}); putting the old one back"));
                let _ = tokio::fs::rename(&old_dir, &final_dir).await;
                if was_loaded {
                    self.bring_back(name, subname, was_enabled).await;
                }
            } else {
                // Nothing was installed before, so there is nothing to remember it by
                let _ = self.store.delete(name, subname).await;
            }
            return Err(format!("it could not be loaded: {why}"));
        }
        let plugin_source = PluginSource { source_type: source.source_type.clone(), url: source.url.clone(), git_ref: source.commit.clone().or_else(|| source.git_ref.clone()), sha256: source.sha256.clone() };
        if let Err(e) = self.store.record_install(name, subname, &manifest.version, manifest.kind.as_str(), &plugin_source).await {
            tracing::warn!("{e}");
        }
        Ok(())
    }

    /// Loads the version that was installed again after an update failed, and switches it on if it was on: the failed load will
    /// have switched a plugin off that it could not register, and the owner's choice is not the update's to change.
    async fn bring_back(&self, name: &str, subname: &str, was_enabled: bool) {
        if let Err(e) = self.load_dir(name, subname).await {
            tracing::error!("the old version of {name}/{subname} could not be loaded again: {}", e.message());
            return;
        }
        if was_enabled {
            if let Err(e) = self.registry.set_enabled(name, subname, true).await {
                tracing::error!("the old version of {name}/{subname} could not be switched on again: {e}");
            }
        }
    }

    /// Removes a plugin: it is unloaded, its folder and its record are deleted. Its settings, its tables and its data folder stay
    /// unless `remove_data` is set, so that installing it again picks up where it left off.
    pub async fn uninstall(&self, name: &str, subname: &str, remove_data: bool) -> Result<(), ManagerError> {
        schema::prefix(name, subname).map_err(|e| ManagerError::Refused(e.to_string()))?;
        let _guard = self.installing.clone().try_lock_owned().map_err(|_| ManagerError::Refused("an install or uninstall is running; wait for it to end".to_string()))?;
        let known = self.store.list().await?.into_iter().any(|r| r.name == name && r.subname == subname);
        let folder = self.root.join(name).join(subname);
        if !known && !folder.is_dir() {
            return Err(ManagerError::NotFound(format!("no external plugin {name}/{subname}")));
        }
        if self.loaded.lock().await.contains_key(&(name.to_string(), subname.to_string())) {
            self.unload(name, subname).await?;
        }
        remove_folder(&folder).await?;
        // The folder named after the plugin goes when it holds nothing else
        let _ = tokio::fs::remove_dir(self.root.join(name)).await;
        self.store.delete(name, subname).await?;
        if remove_data {
            self.registry.drop_database(name, subname).await.map_err(|e| ManagerError::Failed(e.to_string()))?;
            self.registry.forget_settings(name, subname).await.map_err(|e| ManagerError::Failed(e.to_string()))?;
            let data = self.plugins_dir.join(name).join(subname);
            remove_folder(&data).await?;
            let _ = tokio::fs::remove_dir(self.plugins_dir.join(name)).await;
        }
        Ok(())
    }

    /// The plugin's folder as a `.tulpa` file: its name and its bytes
    pub async fn export(&self, name: &str, subname: &str) -> Result<(String, Vec<u8>), ManagerError> {
        schema::prefix(name, subname).map_err(|e| ManagerError::Refused(e.to_string()))?;
        let folder = self.root.join(name).join(subname);
        let manifest = Manifest::load(&folder).map_err(|_| ManagerError::NotFound(format!("no external plugin {name}/{subname} on disk")))?;
        let bytes = tokio::task::spawn_blocking(move || zip_folder(&folder)).await.map_err(|e| ManagerError::Failed(e.to_string()))?.map_err(ManagerError::Failed)?;
        Ok((format!("{name}-{subname}-{}.tulpa", manifest.version), bytes))
    }
}

async fn remove_folder(folder: &Path) -> Result<(), ManagerError> {
    match tokio::fs::remove_dir_all(folder).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ManagerError::Failed(format!("cannot remove {}: {e}", folder.display()))),
    }
}
