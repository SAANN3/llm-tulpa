use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use tool_derive::ToolParams;

use crate::plugins::messaging::plugin::normalize_images;
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError, ATTACHED_IMAGES_KEY,
};
use crate::tools::storage::{check_file_scope, normalize, refuse_if_larger};

const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Added to what goes wrong reaching the file. A model that was shown an image in the conversation
/// can still call this tool for it, naming a path or address it made up; the failure is where it can
/// be told that the image is already in front of it.
const ATTACHED_HINT: &str = " If you meant an image the user attached to their message, you can already see it: \
     look at it directly instead of calling this tool.";

pub struct ReadImageTool;

#[derive(Deserialize, ToolParams)]
struct ReadImageArgs {
    #[tool(description = "Absolute or relative path to the image file to look at.")]
    path: String,
}

#[async_trait]
impl Tool for ReadImageTool {
    fn function_name(&self) -> &str {
        "llm.read_image"
    }

    fn description(&self) -> &str {
        "Shows you an image file from disk: the picture comes into this conversation right after \
         the result, as an image you look at yourself, so what you say about it is what you \
         see (a screenshot of a page you are styling, a chart, a photo). This is only for an \
         image you found or were pointed to by path — not one already shown to you directly in \
         this conversation, which you can already see yourself; calling this on one of those \
         just fills the context with the same picture twice. Check whether you can already see \
         the image before reaching for this. A picture stays in the conversation until the chat \
         is compacted, so ask for it only when you need to look."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        ReadImageArgs::tool_properties()
    }

    // Reads a real file off disk, same permission a plain storage.read_file on this
    // path would need — not a separate grant of its own.
    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(&self, data: Value, scope: ResolvedScope) -> Result<ToolPermission, ToolSerializationError> {
        let args: ReadImageArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(&args.path, SharedBucket::StorageRead, scope.shared.get(&SharedBucket::StorageRead)))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReadImageArgs = serde_json::from_value(data)?;
        if args.path.starts_with("http://") || args.path.starts_with("https://") {
            return Err(ToolError::FailedUnknown(format!(
                "'{}' is an address, not a file path: download it first (web.download_file) and pass the path it was saved to.{ATTACHED_HINT}",
                args.path
            )));
        }
        let path = normalize(std::path::Path::new(&args.path));
        // No model takes an image this large, and reading it whole would only fill memory
        refuse_if_larger(&path, MAX_IMAGE_BYTES, "for an image", "make a smaller copy first (for example with ImageMagick's `convert`)").await?;

        // The picture is kept in the chat, and a model that can't take images would then fail every later
        // request until it was cleared: so it is refused here, before anything is stored
        let provider = ctx.providers.get(&ctx.provider).map_err(|e| {
            ToolError::FailedUnknown(format!("couldn't read the image: {}", e.message.unwrap_or_default()))
        })?;
        if provider.accepts_images(&ctx.model).await == Some(false) {
            return Err(ToolError::FailedUnknown(format!(
                "the model of this chat can't take images (its server reports no vision support), so '{}' can't be shown to it: \
                 tell the user, or use another way to find out what is in it (OCR on the command line, say).",
                path.display()
            )));
        }

        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}.{ATTACHED_HINT}", path.display())))?;

        let attached = normalize_images(vec![STANDARD.encode(&bytes)]).await;
        if attached.is_empty() {
            return Err(ToolError::FailedUnknown(format!("'{}' is not an image that can be decoded (a damaged or unsupported file).", path.display())));
        }

        // What the model reads in place of the picture in the result: the picture itself comes right after,
        // as a message of its own (see `ATTACHED_IMAGES_KEY`)
        let mut result = serde_json::Map::new();
        result.insert("path".to_string(), Value::String(path.display().to_string()));
        result.insert("shown".to_string(), Value::String("The image follows in the next message.".to_string()));
        result.insert(ATTACHED_IMAGES_KEY.to_string(), serde_json::to_value(attached)?);
        Ok(Value::Object(result))
    }
}
