use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, ScopeGrant, SharedBucket, Tool, ToolError, ToolParams,
    ToolPermission, ToolSerializationError,
};
use crate::tools::storage::normalize;

use super::parse_host;

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
        let folder_granted = folders.is_some_and(|f| f.keys().any(|k| target.starts_with(k)));

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

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: DownloadFileArgs = serde_json::from_value(data)?;
        let path = normalize(Path::new(&args.path));

        let response = reqwest::get(&args.url)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't fetch {}: {e}", &args.url)))?;

        let status_code = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|h| h.to_str().unwrap_or("unknown").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let bytes = response
            .bytes()
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read body from {}: {e}", &args.url)))?;

        tokio::fs::write(&path, &bytes)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't write '{}': {e}", path.display())))?;

        Ok(serde_json::to_value(DownloadFileOut {
            path: path.to_string_lossy().to_string(),
            status_code,
            content_type,
            bytes_written: bytes.len() as u64,
        })?)
    }
}
