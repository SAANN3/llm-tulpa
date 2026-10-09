//! Searches Hugging Face for GGUF models and downloads their files into the model folder, as
//! background tasks. A download resumes from what is already on disk and is checked against the
//! sha256 Hugging Face publishes for the file.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use utoipa::ToSchema;

use super::error::ErrorService;
use super::model_folder::{self, ModelFolder};

const API: &str = "https://huggingface.co/api/models";
/// Left free on the disk after a download, so the system around it isn't starved
const DISK_MARGIN_BYTES: u64 = 512 * 1024 * 1024;
const FINISHED_TASK_TTL: Duration = Duration::from_secs(30 * 60);

pub const GATED_MESSAGE: &str = "this model is gated by Hugging Face, to download it you need to set up your token in the settings";

#[derive(Serialize, ToSchema)]
pub struct HfRepo {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    /// When the repository was created, as Hugging Face writes it (RFC 3339)
    pub created_at: Option<String>,
    /// Whether the author makes people accept terms first; downloading needs a token then
    pub gated: bool,
}

/// One page of a search, and where the next one starts
pub struct HfSearchPage {
    pub repos: Vec<HfRepo>,
    pub next_cursor: Option<String>,
}

/// The orders Hugging Face sorts a search by, newest or biggest first: downloads, likes, when it was created, when
/// it last changed
pub const SORTS: [&str; 4] = ["downloads", "likes", "createdAt", "lastModified"];

/// The `cursor` of the `rel="next"` link in a `Link` header (`<https://…&cursor=…>; rel="next"`)
fn next_cursor(link: &str) -> Option<String> {
    let next = link.split(',').find(|part| part.contains(r#"rel="next""#))?;
    let url = next.split(['<', '>']).nth(1)?;
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| pair.strip_prefix("cursor=")).map(str::to_string)
}

#[derive(Serialize, ToSchema)]
pub struct HfFile {
    /// Path inside the repository
    pub path: String,
    pub size_bytes: u64,
    /// A vision projector rather than a language model
    pub projector: bool,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct HfTask {
    pub id: u64,
    pub repo: String,
    pub file: String,
    /// `running`, `done` or `failed`
    pub state: String,
    pub phase: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
    /// Where the file lands, relative to the model folder — what registering it refers to
    pub local_path: String,
    #[serde(skip)]
    finished_at: Option<Instant>,
}

pub struct HfLibrary {
    http: reqwest::Client,
    model_dir: Arc<ModelFolder>,
    tasks: Arc<Mutex<Vec<HfTask>>>,
    next_id: std::sync::atomic::AtomicU64,
}

#[derive(Deserialize)]
struct SearchItem {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(default, rename = "createdAt")]
    created_at: Option<String>,
    #[serde(default)]
    gated: serde_json::Value,
}

#[derive(Deserialize)]
struct TreeItem {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    #[serde(default)]
    size: u64,
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    oid: String,
}

/// A repository id (`owner/name`) or a path inside one: plain segments only, so neither can climb
/// out of the model folder or smuggle a URL.
fn safe_segments(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 300
        && text.split('/').all(|p| {
            !p.is_empty() && p != "." && p != ".." && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '(' | ')' | '+'))
        })
}

fn failed(status: StatusCode, message: impl Into<String>) -> ErrorService {
    ErrorService::new(status, message)
}

impl HfLibrary {
    pub fn new(model_dir: Arc<ModelFolder>) -> Self {
        Self {
            http: reqwest::Client::new(),
            model_dir,
            tasks: Arc::new(Mutex::new(Vec::new())),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    fn get(&self, url: &str, token: Option<&str>) -> reqwest::RequestBuilder {
        let request = self.http.get(url).header("User-Agent", "llm-tulpa");
        match token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    /// One page of repositories with GGUF files, in `sort` order (one of `SORTS`), and the cursor of the
    /// next page when there is one. `cursor` is what an earlier page returned; Hugging Face pages by it.
    pub async fn search(&self, query: &str, sort: &str, cursor: Option<&str>, token: Option<&str>) -> Result<HfSearchPage, ErrorService> {
        if !SORTS.contains(&sort) {
            return Err(failed(StatusCode::BAD_REQUEST, "unknown sort order"));
        }
        let mut params = vec![("search", query), ("filter", "gguf"), ("sort", sort), ("direction", "-1"), ("limit", "30")];
        if let Some(cursor) = cursor {
            params.push(("cursor", cursor));
        }
        let res = self
            .get(API, token)
            .query(&params)
            .send()
            .await
            .map_err(|e| failed(StatusCode::BAD_GATEWAY, format!("could not reach Hugging Face: {e}")))?;
        if !res.status().is_success() {
            return Err(failed(StatusCode::BAD_GATEWAY, format!("Hugging Face answered {}", res.status())));
        }
        let next_cursor = res.headers().get(reqwest::header::LINK).and_then(|link| link.to_str().ok()).and_then(next_cursor);
        let items: Vec<SearchItem> = res.json().await.map_err(|e| failed(StatusCode::BAD_GATEWAY, format!("unexpected answer from Hugging Face: {e}")))?;
        let repos = items
            .into_iter()
            .map(|i| HfRepo {
                gated: !matches!(i.gated, serde_json::Value::Bool(false) | serde_json::Value::Null),
                id: i.id,
                downloads: i.downloads,
                likes: i.likes,
                created_at: i.created_at,
            })
            .collect();
        Ok(HfSearchPage { repos, next_cursor })
    }

    async fn tree(&self, repo: &str, token: Option<&str>) -> Result<Vec<TreeItem>, ErrorService> {
        if !safe_segments(repo) {
            return Err(failed(StatusCode::BAD_REQUEST, "that is not a repository name"));
        }
        let res = self
            .get(&format!("{API}/{repo}/tree/main?recursive=true"), token)
            .send()
            .await
            .map_err(|e| failed(StatusCode::BAD_GATEWAY, format!("could not reach Hugging Face: {e}")))?;
        match res.status() {
            s if s.is_success() => res.json().await.map_err(|e| failed(StatusCode::BAD_GATEWAY, format!("unexpected answer from Hugging Face: {e}"))),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(failed(StatusCode::FORBIDDEN, GATED_MESSAGE)),
            StatusCode::NOT_FOUND => Err(failed(StatusCode::NOT_FOUND, "no such repository")),
            s => Err(failed(StatusCode::BAD_GATEWAY, format!("Hugging Face answered {s}"))),
        }
    }

    pub async fn files(&self, repo: &str, token: Option<&str>) -> Result<Vec<HfFile>, ErrorService> {
        Ok(self
            .tree(repo, token)
            .await?
            .into_iter()
            .filter(|i| i.kind == "file" && i.path.to_lowercase().ends_with(".gguf"))
            .map(|i| HfFile { projector: i.path.to_lowercase().contains("mmproj"), path: i.path, size_bytes: i.size })
            .collect())
    }

    pub fn tasks(&self) -> Vec<HfTask> {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|t| t.finished_at.is_none_or(|at| at.elapsed() < FINISHED_TASK_TTL));
        tasks.clone()
    }

    fn update(tasks: &Mutex<Vec<HfTask>>, id: u64, f: impl FnOnce(&mut HfTask)) {
        if let Some(t) = tasks.lock().unwrap().iter_mut().find(|t| t.id == id) {
            f(t);
        }
    }

    pub async fn start(self: &Arc<Self>, repo: &str, file: &str, token: Option<String>) -> Result<HfTask, ErrorService> {
        let root = self.model_dir.get().ok_or_else(|| failed(StatusCode::CONFLICT, "no model folder is set: choose one on the Models page"))?;
        if !safe_segments(repo) || !safe_segments(file) || !file.to_lowercase().ends_with(".gguf") {
            return Err(failed(StatusCode::BAD_REQUEST, "that is not a .gguf file of a repository"));
        }
        if let Some(running) = self.tasks.lock().unwrap().iter().find(|t| t.state == "running" && t.repo == repo && t.file == file) {
            return Ok(running.clone());
        }
        // The listing tells the size and the sha256, and refuses a gated repository without a token
        let item = self
            .tree(repo, token.as_deref())
            .await?
            .into_iter()
            .find(|i| i.kind == "file" && i.path == file)
            .ok_or_else(|| failed(StatusCode::NOT_FOUND, "no such file in that repository"))?;
        let expected = item.lfs.map(|l| l.oid);

        let local = format!("{repo}/{file}");
        // What is still to be written: a resumed download already has its first part on disk. Refused up
        // front, since a download that fills the disk halfway leaves the whole system short of space.
        let already = std::fs::metadata(format!("{}.part", root.join(&local).display())).map(|m| m.len()).unwrap_or(0);
        let needed = item.size.saturating_sub(already);
        if let Some(free) = model_folder::free_bytes(&root) {
            if needed.saturating_add(DISK_MARGIN_BYTES) > free {
                return Err(failed(
                    StatusCode::CONFLICT,
                    format!("not enough free space in the model folder: {} needed, {} free", gb(needed), gb(free)),
                ));
            }
        }
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let task = HfTask {
            id, repo: repo.into(), file: file.into(), state: "running".into(), phase: "starting".into(),
            completed_bytes: 0, total_bytes: item.size, error: None, local_path: local.clone(), finished_at: None,
        };
        self.tasks.lock().unwrap().push(task.clone());

        let (library, tasks) = (self.clone(), self.tasks.clone());
        let url = format!("https://huggingface.co/{repo}/resolve/main/{file}");
        tokio::spawn(async move {
            let result = library.fetch(&url, &root.join(&local), item.size, expected, token.as_deref(), id).await;
            Self::update(&tasks, id, |t| {
                t.finished_at = Some(Instant::now());
                match result {
                    Ok(()) => {
                        t.state = "done".into();
                        t.phase = "done".into();
                    }
                    Err(e) => {
                        t.state = "failed".into();
                        t.error = Some(e);
                    }
                }
            });
        });
        Ok(task)
    }

    async fn fetch(&self, url: &str, dest: &Path, size: u64, expected: Option<String>, token: Option<&str>, id: u64) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        let part = PathBuf::from(format!("{}.part", dest.display()));
        let mut hasher = Sha256::new();
        let mut have = 0u64;

        // Resume: what is already on disk counts, and goes through the hash first
        if let Ok(mut existing) = tokio::fs::File::open(&part).await {
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = existing.read(&mut buf).await.map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                have += n as u64;
            }
        }
        if size > 0 && have > size {
            let _ = tokio::fs::remove_file(&part).await;
            return Err("the partial file was larger than the model; it was removed, try again".into());
        }

        Self::update(&self.tasks, id, |t| {
            t.phase = "downloading".into();
            t.completed_bytes = have;
        });
        if size == 0 || have < size {
            let mut request = self.get(url, token);
            if have > 0 {
                request = request.header("Range", format!("bytes={have}-"));
            }
            let res = request.send().await.map_err(|e| format!("download failed: {e}"))?;
            match res.status() {
                s if s.is_success() => {}
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Err(GATED_MESSAGE.into()),
                s => return Err(format!("Hugging Face answered {s}")),
            }
            let mut file = tokio::fs::OpenOptions::new().create(true).append(true).open(&part).await.map_err(|e| e.to_string())?;
            let mut stream = res.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| format!("the download broke off (running it again resumes it): {e}"))?;
                hasher.update(&chunk);
                file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                have += chunk.len() as u64;
                Self::update(&self.tasks, id, |t| t.completed_bytes = have);
            }
            file.flush().await.map_err(|e| e.to_string())?;
        }

        Self::update(&self.tasks, id, |t| t.phase = "verifying".into());
        if let Some(expected) = expected {
            let actual = format!("{:x}", hasher.finalize());
            if !actual.eq_ignore_ascii_case(&expected) {
                let _ = tokio::fs::remove_file(&part).await;
                return Err("the file does not match its published sha256 and was discarded; try again".into());
            }
        }
        tokio::fs::rename(&part, dest).await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::safe_segments;

    #[test]
    fn names_that_could_escape_are_refused() {
        assert!(safe_segments("unsloth/Qwen3-GGUF"));
        assert!(safe_segments("sub/model-Q4_K_M.gguf"));
        assert!(!safe_segments("../x"));
        assert!(!safe_segments("/abs/x"));
        assert!(!safe_segments("a//b"));
        assert!(!safe_segments("a/b?x=1"));
    }
}

/// Bytes as "12.3 GB" (decimal, as the app shows sizes everywhere) for an error message
fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}
