use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::services::llm::{CallParams, ChatMessage, ThinkChoice};
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
};
use crate::tools::storage::{check_file_scope, normalize, refuse_if_larger};

const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Used when `prompt` is left empty — a neutral "just tell me what's in it" request
/// rather than requiring the model to always come up with something to ask.
const DEFAULT_PROMPT: &str = "Describe this image in detail: what it shows, any text visible in \
     it, and anything else that seems relevant.";

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
    #[tool(
        description = "What to ask about the image, in your own words — e.g. \"is there a cat \
                        in this picture\", \"transcribe any text in this screenshot\", \"what's \
                        the dominant color scheme\". Leave empty for a general description."
    )]
    prompt: Option<String>,
}

#[derive(Serialize)]
struct ReadImageOut {
    answer: String,
}

#[async_trait]
impl Tool for ReadImageTool {
    fn function_name(&self) -> &str {
        "llm.read_image"
    }

    fn description(&self) -> &str {
        "Looks at an image file on disk and answers a question about it (or gives a general \
         description if no prompt is given). This is only for an image you found or were \
         pointed to by path — not one already shown to you directly in this conversation, \
         which you can already see yourself; calling this on one of those just wastes a call \
         and a real model turn. Check whether you can already see the image before reaching \
         for this."
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

        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}.{ATTACHED_HINT}", path.display())))?;

        let prompt = args
            .prompt
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PROMPT.to_string());

        // A one-shot, tool-free call — this isn't a turn of its own, just this tool
        // asking the (vision-capable) model a single question about one image and
        // relaying the answer back. `think: Some(false)`: nothing here needs a
        // reasoning trace, just a direct answer.
        let message = ChatMessage::user_with_images(prompt, vec![STANDARD.encode(&bytes)]);
        let provider = ctx.providers.get(&ctx.provider).map_err(|e| {
            ToolError::FailedUnknown(format!("couldn't read the image: {}", e.message.unwrap_or_default()))
        })?;
        let response = provider.chat(vec![], Some(message), &[], Some(ThinkChoice::Enabled(false)), &ctx.model, None, &CallParams::default(), None).await.map_err(|e| {
            let reason = match e {
                crate::services::llm::LlmErrors::RequestFailed(_, msg) => msg,
                crate::services::llm::LlmErrors::UnexpectedStatus(provider, status, body) => {
                    format!("{provider} returned status {status}: {body}")
                }
                crate::services::llm::LlmErrors::DecodeFailed(_, msg) => msg,
                crate::services::llm::LlmErrors::Rejected(_, msg) | crate::services::llm::LlmErrors::Failed(msg) => msg,
                crate::services::llm::LlmErrors::Unavailable(err) => err.message.unwrap_or_default(),
            };
            ToolError::FailedUnknown(format!("couldn't read the image: {reason}"))
        })?;

        Ok(serde_json::to_value(ReadImageOut { answer: response.message.content })?)
    }
}
