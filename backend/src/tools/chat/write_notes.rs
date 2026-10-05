use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{PropertyInfo, PropertyType, Tool, ToolContext, ToolError, ToolParams};

/// What the notes may hold: about 2,500 tokens. They ride in front of every request, so the
/// ceiling is what keeps them from becoming a second history; a longer text is refused rather
/// than cut, because the model knows which part matters and a cut would choose for it.
pub const MAX_NOTES_CHARS: usize = 8_000;

pub struct WriteNotesTool;

#[derive(Deserialize, ToolParams)]
struct WriteNotesArgs {
    #[tool(description = "The complete notes text, replacing the previous notes. Plain text; keep what is still true from the old notes. An empty string clears them.")]
    notes: String,
}

#[derive(Serialize)]
struct WriteNotesOut {
    saved_chars: usize,
    max_chars: usize,
    /// Said back so the model doesn't write them again to see them
    note: &'static str,
}

#[async_trait]
impl Tool for WriteNotesTool {
    fn function_name(&self) -> &str {
        "chat.write_notes"
    }

    fn description(&self) -> &str {
        "Saves your working notes for this chat: the plan, what is decided, what is done and what comes \
         next, and the file paths, function names and facts you will need again. The notes replace the \
         previous ones, so pass the whole text you want kept. They come back to you at the top of the \
         request after the conversation is next compacted, and survive every compaction after that; a \
         compaction keeps findings but drops plans and next steps, so this is where those survive. Until \
         then the text you passed stays in view in this call. Check before a long investigation or when a \
         plan has settled whether the notes hold it, and rewrite them when something worth keeping changed. \
         At most 8,000 characters; a longer text is refused, so condense it."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        WriteNotesArgs::tool_properties()
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: WriteNotesArgs = serde_json::from_value(data)?;
        let notes = args.notes.trim();
        let chars = notes.chars().count();
        if chars > MAX_NOTES_CHARS {
            return Err(ToolError::FailedUnknown(format!(
                "the notes are {chars} characters, over the {MAX_NOTES_CHARS} limit; condense them and call again"
            )));
        }
        ctx.chat_store
            .set_pending_notes(ctx.chat_id, notes.to_string())
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't save the notes: {e:?}")))?;
        Ok(serde_json::to_value(WriteNotesOut {
            saved_chars: chars,
            max_chars: MAX_NOTES_CHARS,
            note: "saved; they join the top of the request at the next compaction, the text above stays in view until then",
        })?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The description is a fixed string (the trait wants a `&str`), so the limit in it is written out; this
    /// fails when the constant changes and the text does not, which would tell the model a wrong limit.
    #[test]
    fn the_description_states_the_limit_in_the_constant() {
        let written = format!("{},{:03}", MAX_NOTES_CHARS / 1_000, MAX_NOTES_CHARS % 1_000);
        assert!(WriteNotesTool.description().contains(&format!("At most {written} characters")), "{written}");
    }
}
