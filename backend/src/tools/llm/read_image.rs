use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::services::llm::{OllamaService, ThinkChoice};
use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
};
use crate::tools::storage::{check_file_scope, normalize};

/// Used when `prompt` is left empty — a neutral "just tell me what's in it" request
/// rather than requiring the model to always come up with something to ask.
const DEFAULT_PROMPT: &str = "Describe this image in detail: what it shows, any text visible in \
     it, and anything else that seems relevant.";

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
        let path = normalize(std::path::Path::new(&args.path));

        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?;

        let prompt = args
            .prompt
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_PROMPT.to_string());

        // A one-shot, tool-free call — this isn't a turn of its own, just this tool
        // asking the (vision-capable) model a single question about one image and
        // relaying the answer back. `think: Some(false)`: nothing here needs a
        // reasoning trace, just a direct answer.
        let message = OllamaService::user_message_with_images(prompt, vec![STANDARD.encode(&bytes)]);
        let response = ctx.ollama.chat(vec![], Some(message), &[], Some(ThinkChoice::Enabled(false)), &ctx.model).await.map_err(|e| {
            let reason = match e {
                crate::services::llm::OllamaErrors::RequestFailed(msg) => msg,
                crate::services::llm::OllamaErrors::UnexpectedStatus(status) => {
                    format!("ollama returned status {status}")
                }
                crate::services::llm::OllamaErrors::DecodeFailed(msg) => msg,
                crate::services::llm::OllamaErrors::Rejected(_, msg) | crate::services::llm::OllamaErrors::Failed(msg) => msg,
            };
            ToolError::FailedUnknown(format!("couldn't read the image: {reason}"))
        })?;

        Ok(serde_json::to_value(ReadImageOut { answer: response.message.content })?)
    }
}
