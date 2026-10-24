//! Finds the external plugins on disk and starts one: reads its manifest, runs its `check`, reads its migrations and starts
//! its process under a supervisor. What happens to the result (registering it, recording it) is `manager.rs`'s.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;
use tokio::process::Command;
use tokio::sync::mpsc;

use super::builder::ExternalPluginBuilder;
use super::db::PluginDb;
use super::manifest::{Manifest, expand_command};
use super::supervisor::{Event, Supervisor};
use crate::plugins::schema::role_name;

/// How long a plugin's `check` command may run
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// Why a plugin could not be started, with what was learned of it on the way: its manifest, when that could be read
pub struct StartError {
    pub manifest: Option<Manifest>,
    pub why: String,
}

impl From<String> for StartError {
    fn from(why: String) -> Self {
        Self { manifest: None, why }
    }
}

/// Every `<root>/<name>/<subname>` that holds a `plugin.toml`, in name order so the start-up is the same each time. A
/// folder starting with a dot (the installer's staging area) is not a plugin.
pub fn plugin_folders(root: &Path) -> Vec<PathBuf> {
    let subfolders = |dir: &Path| -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()) && !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.path())
            .collect();
        found.sort();
        found
    };
    subfolders(root).iter().flat_map(|name| subfolders(name)).filter(|dir| dir.join("plugin.toml").is_file()).collect()
}

/// The plugin in `dir`, started and ready to be registered. `plugins_dir` is where every plugin's data folder is, `db` what a
/// plugin that keeps data in the database is given, and `events` where its supervisor reports.
pub async fn start_plugin(dir: &Path, plugins_dir: &Path, db: &DatabaseConnection, events: mpsc::UnboundedSender<Event>) -> Result<ExternalPluginBuilder, StartError> {
    let manifest = Manifest::load(dir).map_err(|e| e.to_string())?;
    let failed = |manifest: &Manifest, why: String| StartError { manifest: Some(manifest.clone()), why };
    let data_dir = plugins_dir.join(&manifest.name).join(&manifest.subname);

    if let Some(check) = &manifest.check {
        run_check(check, dir, &data_dir).await.map_err(|why| failed(&manifest, why))?;
    }

    // Read before the process starts: a migration that cannot be read is found without a program left running
    let migrations = match &manifest.database {
        Some(database) => read_migrations(&dir.join(&database.migrations)).map_err(|why| failed(&manifest, why))?,
        None => vec![],
    };
    let plugin_db = match &manifest.database {
        Some(_) => Some(Arc::new(PluginDb::new(db.clone(), role_name(&manifest.name, &manifest.subname).map_err(|e| failed(&manifest, e.to_string()))?))),
        None => None,
    };

    // A plugin that is off has no process, so a program that was never built would only show when it is switched on. The
    // program is looked for here instead, when the command names a file.
    program_exists(&manifest, dir, &data_dir).map_err(|why| failed(&manifest, why))?;

    let supervisor = Supervisor::new(manifest.clone(), dir.to_path_buf(), data_dir.clone(), plugin_db, events);
    tracing::info!("external plugin {}/{} {} loaded (sdk: {})", manifest.name, manifest.subname, manifest.version, manifest.sdk.as_deref().unwrap_or("not stated"));
    Ok(ExternalPluginBuilder::new(manifest, migrations, supervisor, dir.to_path_buf(), data_dir))
}

/// Whether the program the plugin's `run` command names is there, when it is a path (`{plugin_dir}/target/release/x`); a bare
/// name such as `python3` is looked up on the `PATH` when it is started, so it is not judged here.
pub fn program_exists(manifest: &Manifest, plugin_dir: &Path, data_dir: &Path) -> Result<(), String> {
    let words = expand_command(&manifest.run_command(), plugin_dir, data_dir).map_err(|e| e.to_string())?;
    let program = words.first().ok_or("the run command is empty")?;
    if !program.contains('/') {
        return Ok(());
    }
    let path = if Path::new(program).is_relative() { plugin_dir.join(program) } else { PathBuf::from(program) };
    if path.is_file() { Ok(()) } else { Err(format!("its program {} does not exist; a plugin of kind source has to be built first", path.display())) }
}

/// The `.sql` files of a plugin's migrations folder in the order of their names, one step each. A plugin with no such folder
/// has no steps. The names decide the order, so they start with a number (`0001_items.sql`); a step that has been released
/// is never edited, a change is a new file.
fn read_migrations(folder: &Path) -> Result<Vec<String>, String> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("cannot read the migrations folder {}: {e}", folder.display())),
    };
    let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "sql")).collect();
    files.sort();
    files.iter().map(|file| std::fs::read_to_string(file).map_err(|e| format!("cannot read the migration {}: {e}", file.display()))).collect()
}

/// Runs the manifest's `check` and turns a non-zero exit into an error carrying what it printed ("missing ffmpeg")
pub async fn run_check(check: &str, plugin_dir: &Path, data_dir: &Path) -> Result<(), String> {
    let words = expand_command(check, plugin_dir, data_dir).map_err(|e| format!("check: {e}"))?;
    let (program, args) = words.split_first().ok_or("check is empty")?;
    let output = tokio::time::timeout(CHECK_TIMEOUT, Command::new(program).args(args).current_dir(plugin_dir).kill_on_drop(true).output())
        .await
        .map_err(|_| format!("the check '{check}' did not finish within {} s", CHECK_TIMEOUT.as_secs()))?
        .map_err(|e| format!("the check '{check}' could not run: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let said: String = said.trim().chars().take(500).collect();
    Err(format!("what it needs is missing (the check '{check}' failed){}", if said.is_empty() { String::new() } else { format!(": {said}") }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_name_subname_folders_with_a_manifest_are_plugins() {
        let root = std::env::temp_dir().join(format!("tulpa-loader-{}", std::process::id()));
        for dir in ["a/one", "a/two", "b/three", ".staging/x", "c/.hidden", "d/four"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for with_manifest in ["a/one", "b/three", ".staging/x", "c/.hidden"] {
            std::fs::write(root.join(with_manifest).join("plugin.toml"), "").unwrap();
        }
        let found: Vec<String> = plugin_folders(&root).iter().map(|p| p.strip_prefix(&root).unwrap().display().to_string()).collect();
        assert_eq!(found, ["a/one", "b/three"]);
        assert!(plugin_folders(&root.join("missing")).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
