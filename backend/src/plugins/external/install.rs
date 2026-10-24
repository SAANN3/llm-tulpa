//! The steps of an install that run a program or touch the disk beyond unpacking: the plugin's build, the check of a prebuilt
//! binary, and the trial start that shows the program speaks the protocol before it is put in place.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::manifest::{Manifest, current_target, expand_command};
use super::process::{LogRing, PluginProcess};
use super::services::HostServices;
use super::stage::sha256_of;

/// A build that runs longer than this is ended
const BUILD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// A line of build output is cut to this
const MAX_LINE_CHARS: usize = 2000;

/// Runs the plugin's `build` command in its folder and passes every line it prints (stdout and stderr) to `say`. Fails if the
/// command cannot start, exits non-zero or takes longer than 20 minutes.
pub async fn run_build(command: &str, plugin_dir: &Path, data_dir: &Path, say: impl Fn(String) + Send + Sync + Clone + 'static) -> Result<(), String> {
    let words = expand_command(command, plugin_dir, data_dir).map_err(|e| format!("build: {e}"))?;
    let (program, args) = words.split_first().ok_or("the build command is empty")?;
    let program = if program.contains('/') && Path::new(program).is_relative() { plugin_dir.join(program) } else { PathBuf::from(program) };
    say(format!("$ {command}"));
    let mut child = Command::new(&program)
        .args(args)
        .current_dir(plugin_dir)
        .env("TULPA_PLUGIN_DIR", plugin_dir)
        .env("TULPA_DATA_DIR", data_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("the build could not start ({}): {e}", program.display()))?;
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        readers.push(tokio::spawn(forward(BufReader::new(stdout), say.clone())));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(tokio::spawn(forward(BufReader::new(stderr), say.clone())));
    }
    let status = match tokio::time::timeout(BUILD_TIMEOUT, child.wait()).await {
        Ok(status) => status.map_err(|e| format!("the build could not be waited for: {e}"))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(format!("the build did not finish within {} minutes and was ended", BUILD_TIMEOUT.as_secs() / 60));
        }
    };
    // The last lines of output arrive a moment after the process ends
    for reader in readers {
        let _ = reader.await;
    }
    if status.success() { Ok(()) } else { Err(format!("the build failed ({status})")) }
}

async fn forward<R: tokio::io::AsyncRead + Unpin>(reader: BufReader<R>, say: impl Fn(String)) {
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        say(line.chars().take(MAX_LINE_CHARS).collect());
    }
}

/// Checks the binary a prebuilt plugin ships for this machine against the sha256 its manifest gives, and makes it runnable.
/// Blocking.
pub fn verify_prebuilt(manifest: &Manifest, tree: &Path) -> Result<(), String> {
    let target = current_target();
    let binary = manifest.prebuilt.get(&target).ok_or_else(|| format!("it has no prebuilt binary for {target}"))?;
    let path = tree.join(&binary.path);
    if !path.is_file() {
        return Err(format!("the binary {} is not in the plugin", binary.path));
    }
    let found = sha256_of(&path)?;
    if !found.eq_ignore_ascii_case(&binary.sha256) {
        return Err(format!("the binary {} does not match the checksum in plugin.toml (it is {found})", binary.path));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).map_err(|e| format!("cannot make {} runnable: {e}", binary.path))?;
    }
    Ok(())
}

/// Starts the program once, from where it is staged, to see that it runs and speaks the protocol, and ends it again.
pub async fn trial_start(manifest: &Manifest, tree: &Path, data_dir: &Path) -> Result<(), String> {
    let process = PluginProcess::start(manifest, tree, data_dir, Arc::new(HostServices::default()), Arc::new(LogRing::default())).await?;
    process.shutdown().await;
    Ok(())
}
