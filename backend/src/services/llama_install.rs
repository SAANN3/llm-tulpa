//! Downloads llama.cpp's prebuilt release into the app's folder (or records the user's own
//! binary), as a background task the setup wizard watches.
//!
//! Three sources: the release this version was tested with (`pinned`, sha256-verified against the
//! digests built in), the newest release (`latest`, which may break), or a binary the user points at.

mod hardware;
mod pinned;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use utoipa::ToSchema;

pub use hardware::{detect as detect_hardware, missing_from_output, Gpu, Hardware, MissingLib};
use super::error::ErrorService;
use super::llama_runtime::LlamaRuntime;

const RELEASES_API: &str = "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=1";
const DOWNLOAD_BASE: &str = "https://github.com/ggml-org/llama.cpp/releases/download";

#[derive(Clone, Copy, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Pinned,
    Latest,
    Custom,
}

#[derive(Deserialize, ToSchema)]
pub struct InstallRequest {
    pub channel: Channel,
    /// A build id from `Hardware::builds`; the recommended one when left out
    pub build: Option<String>,
    /// For `custom`: the `llama-server` binary, or the folder holding it
    pub custom_path: Option<String>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct InstallTask {
    /// `running`, `done` or `failed`
    pub state: String,
    pub phase: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
}

/// What is installed, from the `install.json` the installer writes beside the binary.
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct Installed {
    pub tag: String,
    pub build: String,
    /// `pinned`, `latest` or `custom`
    pub source: String,
    pub version: Option<String>,
}

pub struct LlamaInstaller {
    runtime: Arc<LlamaRuntime>,
    dir: PathBuf,
    http: reqwest::Client,
    task: Arc<Mutex<Option<InstallTask>>>,
}

/// Where a build's archive is found, by name: `{t}` is the release tag.
fn spec(os: &str, arch: &str, build: &str) -> Option<(&'static str, &'static str, Option<(&'static str, &'static str)>)> {
    Some(match (os, arch, build) {
        ("linux", "x86_64", "cpu") => ("llama-{t}-bin-ubuntu-x64", ".tar.gz", None),
        ("linux", "x86_64", "vulkan") => ("llama-{t}-bin-ubuntu-vulkan-x64", ".tar.gz", None),
        ("linux", "x86_64", "rocm") => ("llama-{t}-bin-ubuntu-rocm-", "-x64.tar.gz", None),
        ("linux", "x86_64", "cuda12") => ("llama-{t}-bin-ubuntu-cuda-12", "-x64.tar.gz", Some(("cudart-llama-{t}-bin-ubuntu-cuda-12", "-x64.tar.gz"))),
        ("linux", "x86_64", "cuda13") => ("llama-{t}-bin-ubuntu-cuda-13", "-x64.tar.gz", Some(("cudart-llama-{t}-bin-ubuntu-cuda-13", "-x64.tar.gz"))),
        ("linux", "aarch64", "cpu") => ("llama-{t}-bin-ubuntu-arm64", ".tar.gz", None),
        ("linux", "aarch64", "vulkan") => ("llama-{t}-bin-ubuntu-vulkan-arm64", ".tar.gz", None),
        ("windows", "x86_64", "cpu") => ("llama-{t}-bin-win-cpu-x64", ".zip", None),
        ("windows", "x86_64", "vulkan") => ("llama-{t}-bin-win-vulkan-x64", ".zip", None),
        ("windows", "x86_64", "rocm") => ("llama-{t}-bin-win-rocm-", "-x64.zip", None),
        ("windows", "x86_64", "cuda12") => ("llama-{t}-bin-win-cuda-12", "-x64.zip", Some(("cudart-llama-bin-win-cuda-12", "-x64.zip"))),
        ("windows", "x86_64", "cuda13") => ("llama-{t}-bin-win-cuda-13", "-x64.zip", Some(("cudart-llama-bin-win-cuda-13", "-x64.zip"))),
        ("windows", "aarch64", "cpu") => ("llama-{t}-bin-win-cpu-arm64", ".zip", None),
        ("macos", "aarch64", "cpu") => ("llama-{t}-bin-macos-arm64", ".tar.gz", None),
        ("macos", "x86_64", "cpu") => ("llama-{t}-bin-macos-x64", ".tar.gz", None),
        _ => return None,
    })
}

/// One file to fetch: its name, where from, how big, and the sha256 it must have (when known).
struct Download {
    name: String,
    url: String,
    size: u64,
    sha256: Option<String>,
}

fn pick<'a>(names: impl Iterator<Item = &'a str>, tag: &str, (prefix, suffix): (&str, &str)) -> Option<&'a str> {
    let prefix = prefix.replace("{t}", tag);
    let mut matches: Vec<&str> = names.filter(|n| n.starts_with(&prefix) && n.ends_with(suffix)).collect();
    matches.sort();
    matches.into_iter().next()
}

impl LlamaInstaller {
    pub fn new(runtime: Arc<LlamaRuntime>, dir: PathBuf) -> Arc<Self> {
        Arc::new(Self { runtime, dir, http: reqwest::Client::new(), task: Arc::new(Mutex::new(None)) })
    }

    pub fn task(&self) -> Option<InstallTask> {
        self.task.lock().unwrap().clone()
    }

    pub fn installed(&self) -> Option<Installed> {
        serde_json::from_str(&std::fs::read_to_string(self.dir.join("install.json")).ok()?).ok()
    }

    fn update(task: &Mutex<Option<InstallTask>>, f: impl FnOnce(&mut InstallTask)) {
        if let Some(t) = task.lock().unwrap().as_mut() {
            f(t);
        }
    }

    pub fn start(self: &Arc<Self>, request: InstallRequest) -> Result<InstallTask, ErrorService> {
        let bad = |why: String| ErrorService::new(StatusCode::BAD_REQUEST, why);
        let hw = hardware::detect();
        let build = request.build.clone().unwrap_or_else(|| hw.recommended.clone());
        if request.channel != Channel::Custom {
            if !hw.builds.contains(&build) {
                return Err(bad(format!("there is no '{build}' build of llama.cpp for {} {}", hw.os, hw.arch)));
            }
        } else if request.custom_path.as_deref().unwrap_or("").trim().is_empty() {
            return Err(bad("name the llama-server binary to use".into()));
        }
        {
            let mut slot = self.task.lock().unwrap();
            if slot.as_ref().is_some_and(|t| t.state == "running") {
                return Err(ErrorService::new(StatusCode::CONFLICT, "an install is already running"));
            }
            *slot = Some(InstallTask { state: "running".into(), phase: "starting".into(), completed_bytes: 0, total_bytes: 0, error: None });
        }
        let installer = self.clone();
        tokio::spawn(async move {
            let result = installer.run(&request, &hw.os, &hw.arch, &build).await;
            Self::update(&installer.task, |t| match result {
                Ok(()) => {
                    t.state = "done".into();
                    t.phase = "installed".into();
                }
                Err(e) => {
                    tracing::warn!("llama.cpp install failed: {e}");
                    t.state = "failed".into();
                    t.error = Some(e);
                }
            });
        });
        Ok(self.task().expect("just set"))
    }

    async fn run(&self, request: &InstallRequest, os: &str, arch: &str, build: &str) -> Result<(), String> {
        // The binary is about to be replaced
        if !self.runtime.is_external() {
            self.runtime.stop().await.map_err(|e| e.message.unwrap_or_default())?;
        }
        if request.channel == Channel::Custom {
            return self.use_custom(request.custom_path.as_deref().unwrap_or_default()).await;
        }

        // Refused before anything is downloaded
        if self.dir.exists() && !installer_owns(&self.dir) {
            return Err(foreign_folder_message(&self.dir));
        }
        let (tag, downloads) = self.plan(request.channel, os, arch, build).await?;
        let work = sibling(&self.dir, ".download");
        let _ = tokio::fs::remove_dir_all(&work).await;
        tokio::fs::create_dir_all(&work).await.map_err(|e| format!("could not create {}: {e}", work.display()))?;

        let total: u64 = downloads.iter().map(|d| d.size).sum();
        Self::update(&self.task, |t| {
            t.total_bytes = total;
            t.phase = "downloading".into();
        });
        let mut files = Vec::new();
        let mut done_before = 0u64;
        for d in &downloads {
            let path = work.join(&d.name);
            self.download(d, &path, done_before).await?;
            done_before += d.size;
            files.push(path);
        }

        Self::update(&self.task, |t| t.phase = "unpacking".into());
        let (dir, src_tag, src_build) = (self.dir.clone(), tag.clone(), build.to_string());
        let source = if request.channel == Channel::Pinned { "pinned" } else { "latest" }.to_string();
        let work2 = work.clone();
        let binary = tokio::task::spawn_blocking(move || install_files(&files, &work2, &dir)).await.map_err(|e| e.to_string())??;
        let _ = tokio::fs::remove_dir_all(&work).await;

        Self::update(&self.task, |t| t.phase = "checking".into());
        let version = self.version_of(&self.dir.join(&binary)).await?;
        let meta = serde_json::json!({"tag": src_tag, "build": src_build, "source": source, "version": version, "binary": binary});
        tokio::fs::write(self.dir.join("install.json"), meta.to_string()).await.map_err(|e| e.to_string())
    }

    /// The release to take and the files of it for this system.
    async fn plan(&self, channel: Channel, os: &str, arch: &str, build: &str) -> Result<(String, Vec<Download>), String> {
        let (main, runtime) = match spec(os, arch, build) {
            Some((p, s, r)) => ((p, s), r),
            None => return Err(format!("there is no '{build}' build for {os} {arch}")),
        };
        if channel == Channel::Pinned {
            let tag = pinned::PINNED_TAG;
            let names = || pinned::PINNED_DIGESTS.iter().map(|d| d.0);
            let mut out = Vec::new();
            for pat in std::iter::once(main).chain(runtime) {
                let name = pick(names(), tag, pat).ok_or_else(|| format!("the pinned release has no {} asset", pat.0.replace("{t}", tag)))?;
                let (_, size, sha) = pinned::PINNED_DIGESTS.iter().find(|d| d.0 == name).expect("picked from the table");
                out.push(Download { name: name.to_string(), url: format!("{DOWNLOAD_BASE}/{tag}/{name}"), size: *size, sha256: Some(sha.to_string()) });
            }
            return Ok((tag.to_string(), out));
        }

        let releases: Vec<serde_json::Value> = self.http.get(RELEASES_API).header("User-Agent", "llm-tulpa").send().await
            .map_err(|e| format!("could not reach GitHub: {e}"))?.json().await.map_err(|e| format!("unexpected answer from GitHub: {e}"))?;
        let release = releases.first().ok_or("GitHub listed no releases")?;
        let tag = release["tag_name"].as_str().ok_or("the release has no tag")?.to_string();
        let assets = release["assets"].as_array().cloned().unwrap_or_default();
        let names: Vec<&str> = assets.iter().filter_map(|a| a["name"].as_str()).collect();
        let mut out = Vec::new();
        for pat in std::iter::once(main).chain(runtime) {
            let name = pick(names.iter().copied(), &tag, pat).ok_or_else(|| format!("release {tag} has no {} asset", pat.0.replace("{t}", &tag)))?;
            let asset = assets.iter().find(|a| a["name"] == name).expect("picked from the list");
            out.push(Download {
                name: name.to_string(),
                url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
                size: asset["size"].as_u64().unwrap_or(0),
                sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
            });
        }
        Ok((tag, out))
    }

    async fn download(&self, d: &Download, path: &Path, done_before: u64) -> Result<(), String> {
        let res = self.http.get(&d.url).header("User-Agent", "llm-tulpa").send().await.map_err(|e| format!("download of {} failed: {e}", d.name))?;
        if !res.status().is_success() {
            return Err(format!("download of {} failed: HTTP {}", d.name, res.status()));
        }
        let mut file = tokio::fs::File::create(path).await.map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut got = 0u64;
        let mut stream = res.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("download of {} broke off: {e}", d.name))?;
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            got += chunk.len() as u64;
            Self::update(&self.task, |t| t.completed_bytes = done_before + got);
        }
        file.flush().await.map_err(|e| e.to_string())?;
        if let Some(expected) = &d.sha256 {
            let actual = format!("{:x}", hasher.finalize());
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(format!("{} does not match its published sha256 (got {actual}); it was not installed", d.name));
            }
        }
        Ok(())
    }

    async fn use_custom(&self, given: &str) -> Result<(), String> {
        let mut binary = PathBuf::from(given.trim());
        if binary.is_dir() {
            binary = binary.join(if cfg!(windows) { "llama-server.exe" } else { "llama-server" });
        }
        if !binary.is_file() {
            return Err(format!("{} is not a file", binary.display()));
        }
        let version = self.version_of(&binary).await?;
        tokio::fs::create_dir_all(&self.dir).await.map_err(|e| e.to_string())?;
        let meta = serde_json::json!({"tag": "custom", "build": "custom", "source": "custom", "version": version, "binary": binary.display().to_string()});
        tokio::fs::write(self.dir.join("install.json"), meta.to_string()).await.map_err(|e| e.to_string())
    }

    /// Runs `--version`; a failure to even start (a missing library, say) is the install's failure,
    /// with the library named when the loader says which.
    async fn version_of(&self, binary: &Path) -> Result<String, String> {
        let parent = binary.parent().unwrap_or(Path::new("."));
        let key = if cfg!(windows) { "PATH" } else { "LD_LIBRARY_PATH" };
        let out = tokio::process::Command::new(binary).arg("--version").current_dir(parent).env(key, parent).output().await
            .map_err(|e| format!("could not run {}: {e}", binary.display()))?;
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        if let Some(lib) = missing_from_output(&text) {
            return Err(format!("llama-server needs {}, which is not installed: {}", lib.name, lib.hint));
        }
        if !out.status.success() {
            return Err(format!("{} --version failed: {}", binary.display(), text.lines().last().unwrap_or("")));
        }
        Ok(text.lines().find(|l| l.contains("version")).unwrap_or("").trim().to_string())
    }
}

/// Unpacks the archives, merges them into one folder and swaps it in for `dir`. Returns the path of
/// `llama-server` relative to `dir`.
fn install_files(archives: &[PathBuf], work: &Path, dir: &Path) -> Result<String, String> {
    let staged = work.join("staged");
    std::fs::create_dir_all(&staged).map_err(|e| e.to_string())?;
    for (n, archive) in archives.iter().enumerate() {
        let part = work.join(format!("part{n}"));
        std::fs::create_dir_all(&part).map_err(|e| e.to_string())?;
        let name = archive.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
        if name.ends_with(".zip") {
            zip::ZipArchive::new(file).and_then(|mut z| z.extract(&part)).map_err(|e| format!("could not unpack {name}: {e}"))?;
        } else {
            let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
            for entry in tar.entries().map_err(|e| format!("could not read {name}: {e}"))? {
                let mut entry = entry.map_err(|e| e.to_string())?;
                // `unpack_in` refuses anything that would land outside `part`
                entry.unpack_in(&part).map_err(|e| format!("could not unpack {name}: {e}"))?;
            }
        }
        // Archives wrap everything in one folder; the contents are what matter
        let mut root = part.clone();
        loop {
            let entries: Vec<_> = std::fs::read_dir(&root).map_err(|e| e.to_string())?.flatten().collect();
            if entries.len() == 1 && entries[0].path().is_dir() {
                root = entries[0].path();
            } else {
                break;
            }
        }
        for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())?.flatten() {
            let target = staged.join(entry.file_name());
            let _ = std::fs::remove_file(&target);
            std::fs::rename(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    let exe = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let relative = find_file(&staged, exe, 3).ok_or_else(|| format!("the download holds no {exe}"))?;

    // Only a folder this installer made (or an empty one) is replaced: `llama_cpp.dir` may point at
    // somebody's own llama.cpp build, which must never be deleted
    if dir.exists() && !installer_owns(dir) {
        return Err(foreign_folder_message(dir));
    }
    let old = sibling(dir, ".old");
    let _ = std::fs::remove_dir_all(&old);
    if dir.exists() {
        std::fs::rename(dir, &old).map_err(|e| format!("could not replace {}: {e}", dir.display()))?;
    }
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if let Err(e) = std::fs::rename(&staged, dir) {
        // Put the previous install back rather than leave none
        let _ = std::fs::rename(&old, dir);
        return Err(e.to_string());
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

/// Whether `dir` is empty, holds nothing but the installer's own `install.json`, or is a release the
/// installer unpacked (`install.json` says `pinned` or `latest`).
fn installer_owns(dir: &Path) -> bool {
    let unpacked = std::fs::read_to_string(dir.join("install.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|meta| matches!(meta["source"].as_str(), Some("pinned" | "latest")));
    let only_ours = std::fs::read_dir(dir).is_ok_and(|entries| entries.flatten().all(|e| e.file_name() == "install.json"));
    unpacked || only_ours
}

fn foreign_folder_message(dir: &Path) -> String {
    format!(
        "{} already holds files this installer did not put there, so it won't replace them; choose an empty folder as llama_cpp.dir, or use 'my own binary'",
        dir.display()
    )
}

/// `dir` with `suffix` added to its name (`llama` -> `llama.old`), never replacing an extension a
/// folder name already has.
fn sibling(dir: &Path, suffix: &str) -> PathBuf {
    let mut name = dir.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    dir.with_file_name(name)
}

fn find_file(root: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_file() && entry.file_name() == name {
            return path.strip_prefix(root).ok().map(Path::to_path_buf);
        }
        if depth > 0 && path.is_dir() {
            if let Some(found) = find_file(&path, name, depth - 1) {
                return Some(Path::new(&entry.file_name()).join(found));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_assets_are_found_for_every_build() {
        let names = || pinned::PINNED_DIGESTS.iter().map(|d| d.0);
        for (os, arch) in [("linux", "x86_64"), ("linux", "aarch64"), ("windows", "x86_64"), ("macos", "aarch64")] {
            for build in hardware::builds_for(os, arch) {
                let (p, s, rt) = spec(os, arch, &build).unwrap();
                assert!(pick(names(), pinned::PINNED_TAG, (p, s)).is_some(), "{os} {arch} {build}");
                if let Some(rt) = rt {
                    assert!(pick(names(), pinned::PINNED_TAG, rt).is_some(), "runtime {os} {build}");
                }
            }
        }
    }

    #[test]
    fn only_the_installers_own_folder_is_replaced() {
        let base = std::env::temp_dir().join(format!("llm-tulpa-install-test-{}", std::process::id()));
        let own = base.join("own");
        let foreign = base.join("foreign");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&foreign).unwrap();
        assert!(installer_owns(&own), "an empty folder is fine");
        std::fs::write(own.join("install.json"), r#"{"source":"pinned"}"#).unwrap();
        std::fs::write(own.join("llama-server"), "x").unwrap();
        assert!(installer_owns(&own), "an unpacked release is the installer's");
        std::fs::write(foreign.join("llama-server"), "x").unwrap();
        assert!(!installer_owns(&foreign), "somebody's own build is not");
        std::fs::write(foreign.join("install.json"), r#"{"source":"custom"}"#).unwrap();
        assert!(!installer_owns(&foreign), "a custom record doesn't make a folder the installer's");
        assert_eq!(sibling(Path::new("/x/llama.v1"), ".old"), PathBuf::from("/x/llama.v1.old"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cpu_does_not_pick_the_vulkan_asset() {
        let names = ["llama-b1-bin-ubuntu-vulkan-x64.tar.gz", "llama-b1-bin-ubuntu-x64.tar.gz"];
        assert_eq!(pick(names.iter().copied(), "b1", ("llama-{t}-bin-ubuntu-x64", ".tar.gz")), Some("llama-b1-bin-ubuntu-x64.tar.gz"));
    }
}
