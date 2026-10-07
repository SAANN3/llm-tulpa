use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, ScopeGrant, SharedBucket, Tool, ToolContext,
    ToolError, ToolParams, ToolPermission, ToolSerializationError,
};
use crate::tools::storage::{is_within_granted, normalize};

use super::parse_host;

/// How long a download may go without receiving anything before it is given up. No limit on the whole: a
/// large file on a slow line takes as long as it takes.
const DOWNLOAD_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

pub struct DownloadFileTool;

#[derive(Deserialize, tool_derive::ToolParams)]
struct DownloadFileArgs {
    #[tool(description = "The full URL to download (e.g. 'https://example.com/file.pdf'). Must include the scheme (http:// or https://).")]
    url: String,
    #[tool(description = "Absolute or relative path to save the downloaded file to. The containing directory must already exist (use storage.create_directory first if not).")]
    path: String,
}

#[derive(Serialize)]
struct DownloadFileOut {
    path: String,
    status_code: u16,
    content_type: String,
    bytes_written: u64,
    /// For a redirect to another host, which isn't followed: where it points (nothing was written)
    #[serde(skip_serializing_if = "Option::is_none")]
    redirected_to: Option<String>,
}

#[async_trait]
impl Tool for DownloadFileTool {
    fn function_name(&self) -> &str {
        "web.download_file"
    }

    fn description(&self) -> &str {
        "Downloads a URL directly to a file on disk and returns the HTTP status code, content \
         type, and bytes written — the body itself is never returned inline, so this works the \
         same whether the URL points at a text page, an image, an archive, or any other file \
         type. Follow up with storage.detect_file_type to identify what was actually downloaded, \
         and storage.read_file to read it back as text (fails cleanly if it isn't actually UTF-8 \
         text)."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        DownloadFileArgs::tool_properties()
    }

    // The one tool that needs both: its own private bucket for allowed hosts (nothing
    // else shares that concern), plus the same shared folder bucket `storage.write_file`
    // etc. use — a folder already approved for writing is also already approved for a
    // download to land in, and vice versa.
    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageWrite]
    }

    fn uses_own_bucket(&self) -> bool {
        true
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: DownloadFileArgs = serde_json::from_value(data)?;

        let host = match parse_host(&args.url) {
            Some(host) => host,
            None => {
                return Ok(ToolPermission::Denied {
                    reason: format!("couldn't parse a host out of '{}'", args.url),
                    escalation: None,
                });
            }
        };

        let target = normalize(Path::new(&args.path));
        let folder = target.parent().map(Path::to_path_buf).unwrap_or_else(|| target.clone());

        let hosts = scope.own.as_ref().and_then(|s| s.get("hosts")).and_then(|h| h.as_object());
        let folders = scope
            .shared
            .get(&SharedBucket::StorageWrite)
            .and_then(|s| s.get(SharedBucket::StorageWrite.json_key()))
            .and_then(|f| f.as_object());

        let host_granted = hosts.is_some_and(|h| h.contains_key(&host));
        let folder_granted = folders.is_some_and(|f| is_within_granted(&target, f.keys()));

        if host_granted && folder_granted {
            return Ok(ToolPermission::Allowed);
        }

        // Only whichever fact is actually missing, not the existing map re-sent
        // alongside it — `Agent::allow_scope` is what appends each delta to whatever's
        // currently granted, read fresh at persist time. See `storage::check_scope`'s
        // matching comment for why offering the whole accumulated map back here would
        // be unsafe when two denied calls from the same reply both need this bucket.
        let own_delta =
            (!host_granted).then(|| serde_json::json!({ "hosts": { host.clone(): true } }));
        let shared_delta = (!folder_granted).then(|| {
            (
                SharedBucket::StorageWrite,
                serde_json::json!({ SharedBucket::StorageWrite.json_key(): { folder.to_string_lossy(): true } }),
            )
        });

        Ok(ToolPermission::Denied {
            reason: format!(
                "no permission granted covering both host '{host}' and folder '{}'",
                folder.display()
            ),
            escalation: Some(ScopeGrant {
                scope: ResolvedScope { own: own_delta, shared: shared_delta.into_iter().collect() },
                ui_message: format!(
                    "Allow this tool to download from '{host}' into '{}' (including subfolders)?",
                    folder.display()
                ),
            }),
        })
    }

    async fn call_untyped(&self, data: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: DownloadFileArgs = serde_json::from_value(data)?;
        let path = normalize(Path::new(&args.path));

        let mut response = super::client_for(&args.url, None, Some(DOWNLOAD_READ_TIMEOUT))?
            .get(&args.url)
            .send()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't fetch {}: {e}", &args.url)))?;

        let status_code = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|h| h.to_str().unwrap_or("unknown").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        if let Some(location) = super::redirect_location(&response) {
            return Ok(serde_json::to_value(DownloadFileOut {
                path: path.to_string_lossy().to_string(),
                status_code,
                content_type,
                bytes_written: 0,
                redirected_to: Some(location),
            })?);
        }

        let bytes_written = save_body(&mut response, &path).await.map_err(ToolError::FailedUnknown)?;

        Ok(serde_json::to_value(DownloadFileOut {
            path: path.to_string_lossy().to_string(),
            status_code,
            content_type,
            bytes_written,
            redirected_to: None,
        })?)
    }
}

/// Writes a response's body to `path` as it arrives: a large file read into memory first could take the backend
/// down with it. A download that fails partway leaves no file behind, since a half-written one would pass for
/// the real thing. Returns the bytes written.
async fn save_body(response: &mut reqwest::Response, path: &Path) -> Result<u64, String> {
    let mut file = tokio::fs::File::create(path).await.map_err(|e| format!("couldn't write '{}': {e}", path.display()))?;
    let mut written: u64 = 0;
    let copied: Result<(), String> = async {
        while let Some(chunk) = response.chunk().await.map_err(|e| format!("couldn't read the body of {}: {e}", response.url()))? {
            file.write_all(&chunk).await.map_err(|e| format!("couldn't write '{}': {e}", path.display()))?;
            written += chunk.len() as u64;
        }
        file.flush().await.map_err(|e| format!("couldn't write '{}': {e}", path.display()))
    }
    .await;
    if let Err(error) = copied {
        drop(file);
        let _ = tokio::fs::remove_file(path).await;
        return Err(error);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::super::test_server::{ok, serve};
    use super::*;

    fn routes(path: &str, _: u16) -> Option<Vec<u8>> {
        match path {
            "/stall" => None,
            _ => Some(ok(&vec![b'y'; 5 * 1024 * 1024])),
        }
    }

    #[tokio::test]
    async fn a_download_is_written_as_it_arrives_and_a_failed_one_leaves_nothing() {
        let port = serve(routes).await;
        let dir = std::env::temp_dir().join(format!("tulpa-download-test-{port}"));
        std::fs::create_dir_all(&dir).unwrap();

        let url = format!("http://127.0.0.1:{port}/file");
        let path = dir.join("file.bin");
        let mut response = super::super::client_for(&url, None, None).ok().expect("client").get(&url).send().await.unwrap();
        assert_eq!(save_body(&mut response, &path).await.unwrap(), 5 * 1024 * 1024);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 5 * 1024 * 1024);

        let url = format!("http://127.0.0.1:{port}/stall");
        let path = dir.join("stalled.bin");
        let mut response = super::super::client_for(&url, None, Some(std::time::Duration::from_millis(300))).ok().expect("client").get(&url).send().await.unwrap();
        assert!(save_body(&mut response, &path).await.is_err());
        assert!(!path.exists(), "a failed download leaves no partial file");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
