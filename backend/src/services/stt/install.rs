//! Installing a speech-to-text model: the archive is downloaded (checked against the sha256 published with the code),
//! unpacked into a folder of its own and marked as installed. A background task the Settings page watches, like the
//! installs of llama.cpp and of models. The archive is unpacked by this code (the `bzip2` and `tar` crates), not by a
//! program of the system, so it works the same on every platform the backend runs on.
//!
//! A download over a flaky connection is retried, and continues where it broke off: the part that arrived is kept in
//! `.download/` (also when the install fails, so pressing Install again goes on from it) and asked for again with a
//! `Range` header.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::StatusCode;
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use utoipa::ToSchema;

use super::catalog::{self, ModelSpec};
use super::SttService;
use crate::services::error::ErrorService;

pub(super) const DOWNLOAD_BASE: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models";
/// How many times a download is tried before the install fails, and how long it waits before the next (times the number of the try)
const ATTEMPTS: u32 = 5;
const RETRY_WAIT: Duration = Duration::from_secs(3);
/// The unpacking reports how much of the archive it has read each time this much more has been
const UNPACK_STEP: u64 = 4 * 1024 * 1024;
/// The file a finished install leaves in the model's folder; a folder without it is a leftover
const MARKER: &str = ".installed";

#[derive(Clone, Serialize, ToSchema)]
pub struct SttTask {
    /// The model being installed
    pub model: String,
    /// `running`, `done` or `failed`
    pub state: String,
    /// `starting`, `downloading`, `checking`, `unpacking` or `installed`
    pub phase: String,
    /// Bytes of the download so far; while unpacking, how much of the archive has been read
    pub completed_bytes: u64,
    pub total_bytes: u64,
    /// What is going wrong and being tried again, while it is: the reason and which try is next
    pub note: Option<String>,
    pub error: Option<String>,
}

pub(super) fn is_installed(dir: &Path, id: &str) -> bool {
    dir.join(id).join(MARKER).is_file()
}

/// Whether an archive entry is a file the model needs. The archives also carry full-size copies of the networks,
/// sample recordings and notes, which would only take disk.
fn wanted(name: &str) -> bool {
    name.ends_with(".int8.onnx") || name.ends_with("tokens.txt")
}

/// A reader that says how much it has read, every `UNPACK_STEP` bytes
struct Counting<R> {
    inner: R,
    read: u64,
    reported: u64,
    on_progress: Box<dyn FnMut(u64) + Send>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        if self.read - self.reported >= UNPACK_STEP {
            self.reported = self.read;
            (self.on_progress)(self.read);
        }
        Ok(n)
    }
}

/// Unpacks `archive` into `into`, without its top folder and with only the files `wanted` names. An entry that would
/// land outside `into` (an absolute path, `..`) is skipped. `on_progress` is told how much of the archive has been read.
fn unpack(archive: &Path, into: &Path, on_progress: impl FnMut(u64) + Send + 'static) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let reader = Counting { inner: std::io::BufReader::new(file), read: 0, reported: 0, on_progress: Box::new(on_progress) };
    let mut tar = tar::Archive::new(bzip2::read::BzDecoder::new(reader));
    std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
    let mut kept = 0;
    for entry in tar.entries().map_err(|e| format!("the archive could not be read: {e}"))? {
        let mut entry = entry.map_err(|e| format!("the archive is damaged: {e}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let mut parts = path.components();
        parts.next();
        let relative: PathBuf = parts.collect();
        let safe = relative.components().all(|c| matches!(c, Component::Normal(_)));
        let name = relative.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if !safe || relative.components().count() != 1 || !wanted(name) {
            continue;
        }
        entry.unpack(into.join(&relative)).map_err(|e| format!("could not write {name}: {e}"))?;
        kept += 1;
    }
    if kept == 0 {
        return Err("the archive holds no model files".to_string());
    }
    Ok(())
}

/// What a failed request came to, with its causes: `error sending request` alone does not say whether the name could not
/// be found, the connection was refused or it timed out
fn describe(e: reqwest::Error) -> String {
    let mut causes = Vec::new();
    let mut source = std::error::Error::source(&e);
    while let Some(cause) = source {
        causes.push(cause.to_string());
        source = cause.source();
    }
    // The address is not a reason, and the message names it already
    let mut text = e.without_url().to_string();
    for cause in causes {
        text.push_str(": ");
        text.push_str(&cause);
    }
    text
}

/// A download that failed: why, and whether another try may do better
struct Failed {
    reason: String,
    retry: bool,
}

impl SttService {
    pub(super) fn update_task(task: &Mutex<Option<SttTask>>, f: impl FnOnce(&mut SttTask)) {
        if let Some(t) = task.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            f(t);
        }
    }

    /// Starts installing `id` and returns at once; `status` shows how it goes
    pub fn start_install(self: &Arc<Self>, id: &str) -> Result<SttTask, ErrorService> {
        let spec = catalog::find(id).ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, format!("there is no speech model '{id}'")))?;
        if is_installed(&self.dir, spec.id) {
            return Err(ErrorService::new(StatusCode::CONFLICT, format!("{} is already installed", spec.label)));
        }
        {
            let mut slot = self.task.lock().unwrap_or_else(|p| p.into_inner());
            if slot.as_ref().is_some_and(|t| t.state == "running") {
                return Err(ErrorService::new(StatusCode::CONFLICT, "a speech model is already being installed"));
            }
            *slot = Some(SttTask {
                model: spec.id.to_string(),
                state: "running".into(),
                phase: "starting".into(),
                completed_bytes: 0,
                total_bytes: spec.download_bytes,
                note: None,
                error: None,
            });
        }
        let service = self.clone();
        tokio::spawn(async move {
            let result = service.run_install(spec).await;
            Self::update_task(&service.task, |t| {
                t.note = None;
                match result {
                    Ok(()) => {
                        t.state = "done".into();
                        t.phase = "installed".into();
                    }
                    Err(e) => {
                        tracing::warn!("installing the speech model {} failed: {e}", spec.id);
                        t.state = "failed".into();
                        t.error = Some(e);
                    }
                }
            });
        });
        Ok(self.task().expect("just set"))
    }

    async fn run_install(&self, spec: &'static ModelSpec) -> Result<(), String> {
        // Not emptied first: what an earlier try downloaded is continued from
        let work = self.dir.join(".download");
        tokio::fs::create_dir_all(&work).await.map_err(|e| format!("could not create {}: {e}", work.display()))?;
        let archive = work.join(spec.archive);
        Self::update_task(&self.task, |t| t.phase = "downloading".into());
        self.download(spec, &archive).await?;

        Self::update_task(&self.task, |t| t.phase = "checking".into());
        let (checked, expected) = (archive.clone(), spec.sha256);
        let actual = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
            let mut file = std::fs::File::open(checked)?;
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher)?;
            Ok(format!("{:x}", hasher.finalize()))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("could not read the download back: {e}"))?;
        if !actual.eq_ignore_ascii_case(expected) {
            // Not worth continuing from: the next try starts again
            let _ = tokio::fs::remove_file(&archive).await;
            return Err(format!("{} does not match its expected sha256 (got {actual}); it was not installed", spec.archive));
        }

        Self::update_task(&self.task, |t| {
            t.phase = "unpacking".into();
            t.completed_bytes = 0;
        });
        let partial = self.dir.join(format!("{}.partial", spec.id));
        let _ = tokio::fs::remove_dir_all(&partial).await;
        let (from, into, task, total) = (archive.clone(), partial.clone(), self.task.clone(), spec.download_bytes);
        tokio::task::spawn_blocking(move || unpack(&from, &into, move |read| Self::update_task(&task, |t| t.completed_bytes = read.min(total))))
            .await
            .map_err(|e| e.to_string())??;
        std::fs::write(partial.join(MARKER), serde_json::json!({"archive": spec.archive, "sha256": spec.sha256}).to_string()).map_err(|e| e.to_string())?;
        let target = self.dir.join(spec.id);
        let _ = tokio::fs::remove_dir_all(&target).await;
        tokio::fs::rename(&partial, &target).await.map_err(|e| format!("could not move the model into place: {e}"))?;
        let _ = tokio::fs::remove_dir_all(&work).await;
        Ok(())
    }

    /// Gets the archive to `path`, trying again when the connection fails and going on from what has arrived
    async fn download(&self, spec: &ModelSpec, path: &Path) -> Result<(), String> {
        if tokio::fs::metadata(path).await.is_ok_and(|m| m.len() == spec.download_bytes) {
            return Ok(());
        }
        let part = PathBuf::from(format!("{}.part", path.display()));
        let mut tried = 0;
        loop {
            tried += 1;
            match self.fetch(spec, &part).await {
                Ok(()) => break,
                Err(failed) if failed.retry && tried < ATTEMPTS => {
                    let wait = RETRY_WAIT * tried;
                    tracing::warn!("downloading {} failed ({}); trying again in {} s", spec.archive, failed.reason, wait.as_secs());
                    let note = format!("{}. Trying again in {} s ({} of {ATTEMPTS}).", failed.reason, wait.as_secs(), tried + 1);
                    Self::update_task(&self.task, |t| t.note = Some(note));
                    tokio::time::sleep(wait).await;
                }
                // What arrived stays in `.part`: pressing Install again goes on from it
                Err(failed) => return Err(failed.reason),
            }
        }
        Self::update_task(&self.task, |t| t.note = None);
        tokio::fs::rename(&part, path).await.map_err(|e| format!("could not move the download into place: {e}"))
    }

    /// One try at the rest of the archive: from where `part` ends, or from the start when it is empty or the server
    /// does not do ranges
    async fn fetch(&self, spec: &ModelSpec, part: &Path) -> Result<(), Failed> {
        let io = |e: std::io::Error| Failed { reason: format!("could not write the download: {e}"), retry: false };
        let mut have = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
        if have > spec.download_bytes {
            let _ = tokio::fs::remove_file(part).await;
            have = 0;
        }
        if have == spec.download_bytes {
            return Ok(());
        }
        let url = format!("{}/{}", self.download_base, spec.archive);
        let mut request = self.http.get(&url).header("User-Agent", "llm-tulpa");
        if have > 0 {
            request = request.header("Range", format!("bytes={have}-"));
        }
        let res = request.send().await.map_err(|e| Failed { reason: format!("could not reach GitHub: {}", describe(e)), retry: true })?;
        let status = res.status();
        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            let _ = tokio::fs::remove_file(part).await;
            return Err(Failed { reason: "the part already downloaded did not fit".to_string(), retry: true });
        }
        if !status.is_success() {
            let transient = status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::REQUEST_TIMEOUT;
            return Err(Failed { reason: format!("the download of {} failed: HTTP {status}", spec.archive), retry: transient });
        }
        // A 200 to a request for a range means the whole file is coming again
        let resumed = status == StatusCode::PARTIAL_CONTENT && have > 0;
        let mut file = if resumed {
            tokio::fs::OpenOptions::new().append(true).open(part).await.map_err(io)?
        } else {
            tokio::fs::File::create(part).await.map_err(io)?
        };
        let mut got = if resumed { have } else { 0 };
        let mut stream = res.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Failed { reason: format!("the download broke off: {}", describe(e)), retry: true })?;
            file.write_all(&chunk).await.map_err(io)?;
            got += chunk.len() as u64;
            Self::update_task(&self.task, |t| t.completed_bytes = got);
        }
        file.flush().await.map_err(io)?;
        if got < spec.download_bytes {
            return Err(Failed { reason: "the download ended early".to_string(), retry: true });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A .tar.bz2 with a top folder, the files a model needs, ones it doesn't and a path that climbs out
    fn archive(path: &Path) {
        let mut tar = tar::Builder::new(Vec::new());
        let mut add = |name: &str, body: &[u8]| {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            // `set_path` would refuse `..`: it is written into the header as it is
            header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            tar.append(&header, body).unwrap();
        };
        add("model-dir/encoder.int8.onnx", b"encoder");
        add("model-dir/tokens.txt", b"tokens");
        add("model-dir/encoder.onnx", b"the big one");
        add("model-dir/test_wavs/en.wav", b"audio");
        add("model-dir/../escape.int8.onnx", b"nope");
        let bytes = tar.into_inner().unwrap();
        let mut out = bzip2::write::BzEncoder::new(std::fs::File::create(path).unwrap(), bzip2::Compression::fast());
        std::io::Write::write_all(&mut out, &bytes).unwrap();
        out.finish().unwrap();
    }

    #[test]
    fn unpacking_keeps_the_files_a_model_needs_and_nothing_that_lands_outside() {
        let dir = std::env::temp_dir().join(format!("stt-unpack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        archive(&dir.join("a.tar.bz2"));
        unpack(&dir.join("a.tar.bz2"), &dir.join("out"), |_| ()).unwrap();
        let mut names: Vec<_> = std::fs::read_dir(dir.join("out")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, vec!["encoder.int8.onnx", "tokens.txt"]);
        assert!(!dir.join("escape.int8.onnx").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Serves `body` over plain HTTP, understanding `Range`. The first connection is cut after `cut` bytes of the body.
    /// Gives the address and the `Range` header of each request seen.
    async fn serve(body: Vec<u8>, cut: usize) -> (String, Arc<Mutex<Vec<Option<String>>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        tokio::spawn(async move {
            let mut served = 0;
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    if socket.read(&mut byte).await.unwrap() == 0 {
                        break;
                    }
                    head.push(byte[0]);
                }
                let text = String::from_utf8_lossy(&head).to_string();
                let range = text.lines().find_map(|l| l.to_lowercase().strip_prefix("range: bytes=").map(|r| r.trim_end_matches('-').to_string()));
                record.lock().unwrap().push(range.clone());
                let start: usize = range.as_deref().and_then(|r| r.parse().ok()).unwrap_or(0);
                let rest = &body[start..];
                let response = if range.is_some() {
                    format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{}/{}\r\nConnection: close\r\n\r\n", rest.len(), body.len() - 1, body.len())
                } else {
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", rest.len())
                };
                socket.write_all(response.as_bytes()).await.unwrap();
                let end = if served == 0 { cut.min(rest.len()) } else { rest.len() };
                socket.write_all(&rest[..end]).await.unwrap();
                socket.shutdown().await.ok();
                served += 1;
            }
        });
        (address, seen)
    }

    #[tokio::test]
    async fn a_download_that_breaks_off_is_tried_again_from_where_it_got_to() {
        let body: Vec<u8> = (0..300_000u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
        let (base, seen) = serve(body.clone(), 120_000).await;
        let dir = std::env::temp_dir().join(format!("stt-download-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spec: &'static ModelSpec = Box::leak(Box::new(ModelSpec {
            id: "t",
            label: "t",
            description: "",
            languages: "",
            family: catalog::Family::Whisper,
            archive: "t.tar.bz2",
            download_bytes: body.len() as u64,
            sha256: "",
            memory_bytes: 0,
        }));
        let service = SttService::with_download_base(dir.clone(), 1, 0, base);
        *service.task.lock().unwrap() = Some(SttTask { model: "t".into(), state: "running".into(), phase: "downloading".into(), completed_bytes: 0, total_bytes: body.len() as u64, note: None, error: None });
        let path = dir.join("t.tar.bz2");
        service.download(spec, &path).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body, "the file is whole, not the first part twice");
        let ranges = seen.lock().unwrap().clone();
        assert_eq!(ranges.len(), 2, "{ranges:?}");
        assert_eq!(ranges[0], None);
        assert_eq!(ranges[1], Some("120000".to_string()), "the second try asks for the rest only");
        assert_eq!(service.task().unwrap().note, None, "the retry note is gone once it works");
        assert!(!dir.join("t.tar.bz2.part").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unpacking_says_how_far_it_has_read() {
        let dir = std::env::temp_dir().join(format!("stt-unpack-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Several megabytes that do not compress, so the archive is about as big as what is in it
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let body: Vec<u8> = (0..9_000_000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 32) as u8
            })
            .collect();
        let mut tar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_path("m/encoder.int8.onnx").unwrap();
        header.set_mode(0o644);
        header.set_cksum();
        tar.append(&header, &body[..]).unwrap();
        let mut out = bzip2::write::BzEncoder::new(std::fs::File::create(dir.join("a.tar.bz2")).unwrap(), bzip2::Compression::fast());
        std::io::Write::write_all(&mut out, &tar.into_inner().unwrap()).unwrap();
        out.finish().unwrap();
        let size = std::fs::metadata(dir.join("a.tar.bz2")).unwrap().len();
        let reported = Arc::new(Mutex::new(Vec::new()));
        let log = reported.clone();
        unpack(&dir.join("a.tar.bz2"), &dir.join("out"), move |read| log.lock().unwrap().push(read)).unwrap();
        let reported = reported.lock().unwrap().clone();
        assert!(reported.len() >= 1, "{reported:?}");
        assert!(reported.windows(2).all(|w| w[0] < w[1]) && *reported.last().unwrap() <= size, "{reported:?} of {size}");
        assert_eq!(std::fs::metadata(dir.join("out/encoder.int8.onnx")).unwrap().len(), body.len() as u64);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
