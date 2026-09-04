use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolError, ToolParams, ToolPermission,
    ToolSerializationError,
};

use super::{check_file_scope, normalize};

pub struct DetectFileTypeTool;

/// Only the first chunk of the file is ever read — magic-byte signatures live in the
/// first few dozen bytes at most, and reading no more than this keeps sniffing a huge
/// file (a video, a big archive) cheap instead of pulling the whole thing into memory.
const SNIFF_BYTES: usize = 8192;

#[derive(Deserialize, ToolParams)]
struct DetectFileTypeArgs {
    #[tool(description = "Absolute or relative path to the file to identify.")]
    path: String,
}

#[derive(Serialize)]
struct DetectFileTypeOut {
    kind: String,
    mime_type: Option<String>,
    extension: Option<String>,
}

#[async_trait]
impl Tool for DetectFileTypeTool {
    fn function_name(&self) -> &str {
        "storage.detect_file_type"
    }

    fn description(&self) -> &str {
        "Identifies what kind of file this actually is by sniffing its first bytes (magic \
         numbers) rather than trusting its extension or name — the right first call on a file \
         you don't already know the format of (e.g. something storage.write_file or \
         web.download_file just produced). Returns a general `kind` (image, video, audio, \
         archive, doc, font, or text), plus a MIME type and typical extension when a specific \
         binary format is recognized. A file with no recognized binary signature that's also \
         valid UTF-8 is reported as `kind: \"text\"` (mime_type/extension left null); anything \
         else is `kind: \"unknown\"`."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        DetectFileTypeArgs::tool_properties()
    }

    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: DetectFileTypeArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(&args.path, SharedBucket::StorageRead, scope.shared.get(&SharedBucket::StorageRead)))
    }

    async fn call_untyped(&self, data: Value) -> Result<Value, ToolError> {
        let args: DetectFileTypeArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));

        let mut file = tokio::fs::File::open(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't open '{}': {e}", path.display())))?;

        let mut buf = vec![0u8; SNIFF_BYTES];
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?;
        buf.truncate(n);

        let out = match infer::get(&buf) {
            Some(kind) => DetectFileTypeOut {
                kind: format!("{:?}", kind.matcher_type()).to_lowercase(),
                mime_type: Some(kind.mime_type().to_string()),
                extension: Some(kind.extension().to_string()),
            },
            // No recognized binary signature — fall back to "is this (start of the file, at
            // least) valid UTF-8 text". A char boundary cut mid-buffer can produce a false
            // "unknown" on a genuinely-text file right at the SNIFF_BYTES edge; storage.read_file
            // is the actual source of truth if this matters for a specific file.
            None if std::str::from_utf8(&buf).is_ok() => DetectFileTypeOut {
                kind: "text".to_string(),
                mime_type: None,
                extension: None,
            },
            None => DetectFileTypeOut {
                kind: "unknown".to_string(),
                mime_type: None,
                extension: None,
            },
        };

        Ok(serde_json::to_value(out)?)
    }
}
