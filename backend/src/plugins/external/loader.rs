//! Finds the external plugins on disk and starts them. A plugin that cannot be started is logged and left out: one broken
//! folder never keeps the backend, or the other plugins, from coming up.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;

use super::builder::ExternalPluginBuilder;
use super::manifest::{Manifest, expand_command};
use super::process::PluginProcess;
use crate::plugins::base::PluginBuilder;

/// How long a plugin's `check` command may run
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// Starts every plugin in `<root>/<name>/<subname>/` and returns a builder for each that came up. `taken` holds the
/// `(name, subname)` of the plugins that are already registered (the ones compiled in): an external plugin with the same
/// name is refused, since two plugins under one key would stop the whole backend at registration. `plugins_dir` is where
/// every plugin's data folder is.
pub async fn load_all(root: &Path, plugins_dir: &Path, taken: &HashSet<(String, String)>) -> Vec<Arc<dyn PluginBuilder>> {
    let mut seen = taken.clone();
    let mut builders: Vec<Arc<dyn PluginBuilder>> = Vec::new();
    for dir in plugin_folders(root) {
        let label = dir.strip_prefix(root).unwrap_or(&dir).display().to_string();
        match load_one(&dir, plugins_dir, &mut seen).await {
            Ok(builder) => builders.push(Arc::new(builder)),
            Err(why) => tracing::error!("external plugin {label} is not available: {why}"),
        }
    }
    builders
}

/// Every `<root>/<name>/<subname>` that holds a `plugin.toml`, in name order so the start-up is the same each time. A
/// folder starting with a dot (the installer's staging area) is not a plugin.
fn plugin_folders(root: &Path) -> Vec<PathBuf> {
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

async fn load_one(dir: &Path, plugins_dir: &Path, seen: &mut HashSet<(String, String)>) -> Result<ExternalPluginBuilder, String> {
    let manifest = Manifest::load(dir).map_err(|e| e.to_string())?;
    let key = (manifest.name.clone(), manifest.subname.clone());
    if seen.contains(&key) {
        return Err(format!("{}/{} is already taken by another plugin", key.0, key.1));
    }
    let data_dir = plugins_dir.join(&manifest.name).join(&manifest.subname);

    if let Some(check) = &manifest.check {
        run_check(check, dir, &data_dir).await?;
    }

    // Its tools are asked for when it is configured (they may depend on the settings), not here
    let process = PluginProcess::start(&manifest, dir, &data_dir).await?;
    tracing::info!("external plugin {}/{} {} started (sdk: {})", manifest.name, manifest.subname, manifest.version, manifest.sdk.as_deref().unwrap_or("not stated"));
    seen.insert(key);
    Ok(ExternalPluginBuilder::new(manifest, process))
}

/// Runs the manifest's `check` and turns a non-zero exit into an error carrying what it printed ("missing ffmpeg")
async fn run_check(check: &str, plugin_dir: &Path, data_dir: &Path) -> Result<(), String> {
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
