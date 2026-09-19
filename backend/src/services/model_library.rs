use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use regex::Regex;
use serde::Serialize;
use utoipa::ToSchema;

use crate::services::error::ErrorService;
use crate::services::llm::{ImportProgress, OllamaService};

/// How long a fetched catalog is reused. The library changes slowly, and opening the model
/// picker shouldn't cost a round trip to ollama.com every time.
const CATALOG_TTL: Duration = Duration::from_secs(60 * 60);

/// How long a finished task stays listed, so a client that wasn't polling when it ended still
/// sees how it went.
const FINISHED_TASK_TTL: Duration = Duration::from_secs(60 * 60);

/// How deep below the model directory local files are looked for. Enough for the usual
/// `<model dir>/<repo>/<file>.gguf` layouts without walking an entire disk.
const MAX_SCAN_DEPTH: usize = 3;

/// One entry of Ollama's public model library.
#[derive(Serialize, ToSchema, Clone)]
pub struct CatalogModel {
    /// The pullable name, e.g. `qwen3`.
    pub name: String,
    pub description: Option<String>,
    /// Feature tags the library lists for it: `tools`, `vision`, `thinking`, ...
    pub capabilities: Vec<String>,
    /// Parameter sizes it comes in, e.g. `8b`.
    pub sizes: Vec<String>,
    /// Download count as the library shows it, e.g. `119.7M`.
    pub pulls: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Catalog {
    /// `false` when ollama.com couldn't be reached and this is the built-in short list.
    pub live: bool,
    pub models: Vec<CatalogModel>,
}

#[derive(Serialize, ToSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalFileKind {
    Model,
    /// A vision projector (`mmproj`) — not a model on its own, paired with one to add vision.
    Projector,
}

/// A `.gguf` file found in the model directory.
#[derive(Serialize, ToSchema)]
pub struct LocalFile {
    /// Path relative to the model directory — what an import refers to it by.
    pub path: String,
    pub size_bytes: u64,
    pub kind: LocalFileKind,
}

#[derive(Serialize, ToSchema)]
pub struct LocalFiles {
    /// Whether a model directory is configured at all (`model_dir` in settings.json).
    pub configured: bool,
    pub files: Vec<LocalFile>,
}

#[derive(Serialize, ToSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Pull,
    Import,
}

#[derive(Serialize, ToSchema, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Running,
    Done,
    Failed,
}

/// A model download or import running in the background. Both take long enough that the
/// request that starts one returns immediately and the client watches this instead.
#[derive(Serialize, ToSchema, Clone)]
pub struct ModelTask {
    pub id: u64,
    pub kind: TaskKind,
    /// The model this produces (its name in Ollama once done).
    pub model: String,
    pub state: TaskState,
    /// What it's doing right now, e.g. `pulling`, `hashing 1/2`.
    pub phase: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    /// Why it failed, when `state` is `failed`.
    pub error: Option<String>,
    #[serde(skip)]
    finished_at: Option<Instant>,
}

/// One requested import.
pub struct ImportRequest {
    /// The model file, relative to the model directory.
    pub file: String,
    /// The name to create it under; derived from the file name when omitted.
    pub name: Option<String>,
    /// A vision projector file, relative to the model directory.
    pub projector: Option<String>,
}

/// Everything about which models can be had: the public library to pull from, the `.gguf` files
/// already on disk to import, and the background tasks doing either. Talks to Ollama only through
/// `OllamaService`; works without a database, so it lives beside `AppState`, not `AppServices`.
pub struct ModelLibrary {
    ollama: Arc<OllamaService>,
    model_dir: Option<PathBuf>,
    catalog: tokio::sync::Mutex<Option<(Instant, Vec<CatalogModel>)>>,
    tasks: Arc<Mutex<Vec<ModelTask>>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl ModelLibrary {
    pub fn new(ollama: Arc<OllamaService>, model_dir: Option<PathBuf>) -> Self {
        Self {
            ollama,
            model_dir,
            catalog: tokio::sync::Mutex::new(None),
            tasks: Arc::new(Mutex::new(Vec::new())),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    // ---- catalog ----

    /// Ollama's public library as structured data. Falls back to a short built-in list when
    /// ollama.com is unreachable or its page no longer parses, so the picker always has
    /// something to show.
    pub async fn catalog(&self) -> Catalog {
        let mut cached = self.catalog.lock().await;
        if let Some((fetched_at, models)) = cached.as_ref() {
            if fetched_at.elapsed() < CATALOG_TTL {
                return Catalog { live: true, models: models.clone() };
            }
        }

        match self.ollama.fetch_catalog().await.map(|html| parse_catalog(&html)) {
            Ok(models) if !models.is_empty() => {
                *cached = Some((Instant::now(), models.clone()));
                Catalog { live: true, models }
            }
            other => {
                match other {
                    Ok(_) => tracing::warn!("ollama.com/library parsed to no models; using the built-in list"),
                    Err(e) => tracing::warn!("could not fetch ollama.com/library ({}); using the built-in list", describe(&e)),
                }
                Catalog { live: false, models: fallback_catalog() }
            }
        }
    }

    // ---- local files ----

    /// The `.gguf` files under the model directory, projectors marked as such.
    pub fn local_files(&self) -> LocalFiles {
        let Some(root) = &self.model_dir else {
            return LocalFiles { configured: false, files: Vec::new() };
        };

        let mut files = Vec::new();
        scan(root, root, 0, &mut files);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        LocalFiles { configured: true, files }
    }

    /// Resolves a path the client sent (relative to the model directory) to a real `.gguf` file
    /// inside it. The client is trusted with *which* file, not with reaching outside the
    /// directory: absolute paths and `..` are refused outright, and the canonical result must
    /// still sit under the canonical root (which also rules out symlinks pointing elsewhere).
    fn resolve_local(&self, relative: &str) -> Result<PathBuf, ErrorService> {
        let bad = |why: &str| ErrorService::new(StatusCode::BAD_REQUEST, format!("'{relative}': {why}"));
        let root = self
            .model_dir
            .as_ref()
            .ok_or_else(|| ErrorService::new(StatusCode::CONFLICT, "no model directory is configured (model_dir in settings.json)"))?;

        let candidate = Path::new(relative);
        if candidate.is_absolute() || candidate.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(bad("must be a path inside the model directory"));
        }
        if candidate.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() != Some("gguf") {
            return Err(bad("not a .gguf file"));
        }

        let full = std::fs::canonicalize(root.join(candidate)).map_err(|_| bad("no such file"))?;
        let root = std::fs::canonicalize(root).map_err(|_| bad("the model directory is not readable"))?;
        if !full.starts_with(&root) || !full.is_file() {
            return Err(bad("must be a file inside the model directory"));
        }
        Ok(full)
    }

    // ---- tasks ----

    pub fn tasks(&self) -> Vec<ModelTask> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|t| t.finished_at.is_none_or(|at| at.elapsed() < FINISHED_TASK_TTL));
        tasks.clone()
    }

    fn add_task(&self, kind: TaskKind, model: String, phase: &str, total_bytes: u64) -> ModelTask {
        let task = ModelTask {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            kind,
            model,
            state: TaskState::Running,
            phase: phase.to_string(),
            completed_bytes: 0,
            total_bytes,
            error: None,
            finished_at: None,
        };
        self.tasks.lock().unwrap().push(task.clone());
        task
    }

    fn update_task(tasks: &Mutex<Vec<ModelTask>>, id: u64, change: impl FnOnce(&mut ModelTask)) {
        if let Some(task) = tasks.lock().unwrap().iter_mut().find(|t| t.id == id) {
            change(task);
        }
    }

    fn finish(tasks: &Mutex<Vec<ModelTask>>, id: u64, result: Result<(), String>) {
        Self::update_task(tasks, id, |task| {
            task.finished_at = Some(Instant::now());
            match result {
                Ok(()) => {
                    task.state = TaskState::Done;
                    task.phase = "done".to_string();
                    task.completed_bytes = task.total_bytes;
                }
                Err(message) => {
                    task.state = TaskState::Failed;
                    task.error = Some(message);
                }
            }
        });
    }

    /// Starts pulling `name` in the background — a library name or tag (`qwen3:8b`), or a
    /// Hugging Face GGUF as `hf.co/<user>/<repo>[:<quant>]`. Returns the task to watch; if the
    /// same model is already being pulled, that task is returned instead of a second one.
    pub fn start_pull(&self, name: &str) -> Result<ModelTask, ErrorService> {
        let name = name.trim();
        if name.is_empty() || name.len() > 200 || name.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, "that isn't a model name"));
        }

        if let Some(running) = self
            .tasks
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.kind == TaskKind::Pull && t.state == TaskState::Running && t.model == name)
        {
            return Ok(running.clone());
        }

        let task = self.add_task(TaskKind::Pull, name.to_string(), "starting", 0);
        let (ollama, tasks, id, model) = (self.ollama.clone(), self.tasks.clone(), task.id, name.to_string());

        tokio::spawn(async move {
            // A pull reports each layer under its own digest; the whole is the sum of them.
            let mut layers: HashMap<String, (u64, u64)> = HashMap::new();
            let result = ollama
                .pull_streaming(&model, |event| {
                    if let (Some(digest), Some(total)) = (event.digest.clone(), event.total) {
                        layers.insert(digest, (total, event.completed.unwrap_or(0)));
                    }
                    let (total, completed) = layers.values().fold((0, 0), |(t, c), (lt, lc)| (t + lt, c + lc));
                    Self::update_task(&tasks, id, |task| {
                        // Layer lines read "pulling <digest>" — noise to a person; phase lines
                        // like "verifying sha256 digest" already read fine.
                        task.phase = if event.digest.is_some() && event.status.starts_with("pulling") {
                            "downloading".to_string()
                        } else {
                            event.status.clone()
                        };
                        task.total_bytes = total;
                        task.completed_bytes = completed;
                    });
                })
                .await;
            Self::finish(&tasks, id, result.map_err(|e| describe(&e)));
        });

        Ok(task)
    }

    /// Starts importing local files as models, one background task each. Every request is
    /// validated up front so a typo fails the whole call before anything starts.
    pub async fn start_imports(&self, requests: Vec<ImportRequest>) -> Result<Vec<ModelTask>, ErrorService> {
        let installed: Vec<String> = self
            .ollama
            .list_local_models()
            .await
            .map(|models| models.into_iter().map(|m| m.name).collect())
            .unwrap_or_default();

        struct Prepared {
            name: String,
            files: Vec<PathBuf>,
            total: u64,
        }
        let mut prepared = Vec::new();
        for request in requests {
            let model_file = self.resolve_local(&request.file)?;
            let name = match &request.name {
                Some(name) => valid_model_name(name)?,
                None => default_model_name(&request.file),
            };
            if installed.iter().any(|n| n == &name || n == &format!("{name}:latest")) {
                return Err(ErrorService::new(
                    StatusCode::CONFLICT,
                    format!("a model called '{name}' is already installed — pick another name"),
                ));
            }
            if prepared.iter().any(|p: &Prepared| p.name == name) {
                return Err(ErrorService::new(StatusCode::BAD_REQUEST, format!("'{name}' is requested twice")));
            }

            let mut files = vec![model_file];
            if let Some(projector) = &request.projector {
                files.push(self.resolve_local(projector)?);
            }
            let total = files.iter().filter_map(|f| std::fs::metadata(f).ok()).map(|m| m.len()).sum();
            prepared.push(Prepared { name, files, total });
        }

        let mut started = Vec::new();
        for Prepared { name, files, total } in prepared {
            let task = self.add_task(TaskKind::Import, name.clone(), "starting", total);
            started.push(task.clone());

            let (ollama, tasks, id) = (self.ollama.clone(), self.tasks.clone(), task.id);
            tokio::spawn(async move {
                let progress = Arc::new(ImportProgress::new());
                let sizes: Vec<u64> = files
                    .iter()
                    .map(|f| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0))
                    .collect();

                // Reports on the import while it runs; stopped as soon as it returns.
                let watcher = {
                    let (progress, tasks) = (progress.clone(), tasks.clone());
                    tokio::spawn(async move {
                        loop {
                            let index = progress.file_index.load(Ordering::Relaxed);
                            let (phase, done) = (progress.phase(), progress.done.load(Ordering::Relaxed));
                            Self::update_task(&tasks, id, |task| {
                                task.phase = if phase == "creating" {
                                    phase.to_string()
                                } else {
                                    format!("{phase} {}/{}", index + 1, sizes.len())
                                };
                                task.total_bytes = sizes.get(index).copied().unwrap_or(0);
                                task.completed_bytes = done;
                            });
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    })
                };

                let result = ollama.import_files(&name, &files, &progress).await;
                watcher.abort();
                Self::finish(&tasks, id, result.map_err(|e| describe(&e)));
            });
        }
        Ok(started)
    }
}

/// The message a person should see for an Ollama failure.
fn describe(err: &crate::services::llm::OllamaErrors) -> String {
    use crate::services::llm::OllamaErrors::*;
    match err {
        RequestFailed(msg) => format!("could not reach Ollama: {msg}"),
        UnexpectedStatus(code) => format!("Ollama returned status {code}"),
        DecodeFailed(msg) | Failed(msg) => msg.clone(),
        Rejected(_, msg) => msg.clone(),
    }
}

/// Recursively collects `.gguf` files under `dir`, skipping hidden entries (which keeps
/// Ollama's own data directory, when it sits inside the model directory, out of the list).
fn scan(root: &Path, dir: &Path, depth: usize, out: &mut Vec<LocalFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };

        if meta.is_dir() {
            if depth < MAX_SCAN_DEPTH {
                scan(root, &path, depth + 1, out);
            }
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("gguf")) {
            let Ok(relative) = path.strip_prefix(root) else { continue };
            let relative = relative.to_string_lossy().replace('\\', "/");
            let kind = if relative.to_ascii_lowercase().contains("mmproj") {
                LocalFileKind::Projector
            } else {
                LocalFileKind::Model
            };
            out.push(LocalFile { path: relative, size_bytes: meta.len(), kind });
        }
    }
}

/// A model name Ollama will accept: lowercase letters, digits, `.`, `_` and `-`, with an
/// optional `:tag`.
fn valid_model_name(name: &str) -> Result<String, ErrorService> {
    let name = name.trim();
    let re = Regex::new(r"^[a-z0-9][a-z0-9._-]*(:[a-z0-9][a-z0-9._-]*)?$").expect("static regex");
    if name.len() <= 100 && re.is_match(name) {
        Ok(name.to_string())
    } else {
        Err(ErrorService::new(
            StatusCode::BAD_REQUEST,
            "a model name may only use lowercase letters, digits, '.', '_' and '-' (plus an optional ':tag')",
        ))
    }
}

/// A model name derived from a file's path: its stem, lowercased, with anything Ollama
/// wouldn't take turned into `-`.
fn default_model_name(relative: &str) -> String {
    let stem = Path::new(relative).file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    let cleaned: String = stem
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect();
    let cleaned = cleaned.trim_matches(|c: char| !c.is_ascii_alphanumeric()).to_string();
    if cleaned.is_empty() { "model".to_string() } else { cleaned }
}

// ---- catalog parsing ----

/// Pulls the model list out of ollama.com/library's HTML. The page has no API behind it, so this
/// reads the markup it renders: one `<li>` per model holding a `/library/<name>` link, a
/// description paragraph, and tag spans (indigo for capabilities, blue for sizes). Kept to the
/// backend so a markup change is fixed in one place and the frontend only ever sees JSON.
pub fn parse_catalog(html: &str) -> Vec<CatalogModel> {
    let item = Regex::new(r#"(?s)<li\b[^>]*>\s*<a href="/library/([^"/?#]+)"(.*?)</li>"#).expect("static regex");
    let description = Regex::new(r#"(?s)<p class="max-w-lg[^"]*">(.*?)</p>"#).expect("static regex");
    let capability = Regex::new(r#"(?s)<span[^>]*bg-indigo-50[^>]*>\s*([^<]+?)\s*</span>"#).expect("static regex");
    let size = Regex::new(r#"(?s)<span[^>]*bg-\[#ddf4ff\][^>]*>\s*([^<]+?)\s*</span>"#).expect("static regex");
    let pulls = Regex::new(r#"<span\s*>\s*([\d.]+[KMB]?)\s*</span>\s*<span[^>]*>&nbsp;Pulls"#).expect("static regex");

    let mut seen = std::collections::HashSet::new();
    let mut models = Vec::new();
    for caps in item.captures_iter(html) {
        let name = caps[1].trim().to_string();
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        let body = &caps[2];
        models.push(CatalogModel {
            name,
            description: description
                .captures(body)
                .map(|c| decode_entities(c[1].trim()))
                .filter(|d| !d.is_empty()),
            capabilities: capability.captures_iter(body).map(|c| c[1].to_string()).collect(),
            sizes: size.captures_iter(body).map(|c| c[1].to_string()).collect(),
            pulls: pulls.captures(body).map(|c| c[1].to_string()),
        });
    }
    models
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Shown when the live library can't be had.
fn fallback_catalog() -> Vec<CatalogModel> {
    [
        ("llama3.2", "Meta's compact Llama 3.2 (1B / 3B)"),
        ("qwen2.5", "Alibaba Qwen 2.5, strong tool-calling"),
        ("qwen3", "Qwen 3, reasoning + tool-calling"),
        ("gemma3", "Google's Gemma 3"),
        ("phi4", "Microsoft's Phi-4"),
        ("mistral", "Mistral 7B"),
        ("deepseek-r1", "DeepSeek R1 reasoning models"),
        ("llava", "Vision-capable multimodal model"),
    ]
    .into_iter()
    .map(|(name, description)| CatalogModel {
        name: name.to_string(),
        description: Some(description.to_string()),
        capabilities: Vec::new(),
        sizes: Vec::new(),
        pulls: None,
    })
    .collect()
}
