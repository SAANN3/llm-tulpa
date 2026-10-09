//! The model's own notes for a chat: where they go in the prompt, and the request that asks for them
//! right before a compaction fold drops the plans from the history.
//!
//! A summary keeps findings and leaves out plans and next steps (deliberately, see `summarize`), so a
//! plan survives a fold only in the notes. The model writes them with `chat.write_notes`, which
//! saves them as pending (changing the front of the prompt would make the model server read the whole
//! conversation again); they join the prompt at the next compaction.

use super::super::prompts;
use super::Compaction;
use crate::services::error::ErrorService;
use crate::services::llm::{ChatMessage, ThinkChoice};
use crate::tools::base::Tool;
use crate::tools::chat::write_notes::MAX_NOTES_CHARS;
use crate::tools::subagent;

/// The notes request is only made with at least this many tokens of the window free: the reply may use what the
/// window leaves after the measured prompt and a margin, and a model that thinks first needs several thousand
/// tokens to reason and then write notes of up to 8,000 characters (about 2,500 tokens). Below this it runs out
/// of room mid-reasoning (measured at a 49k window with 10k free, when the tool definitions were counted twice in the
/// cap: 4,096 tokens, 150 s, no notes). At the compaction trigger a window has 30% free: 12k tokens at 40k.
const NOTES_ASK_MIN_ROOM_TOKENS: u64 = 6_000;

/// The notes a pre-fold reply carries, or `None` when it carries none to store: an empty reply,
/// UNCHANGED, a tool call (a real one or the model's own tags in the text), or text over the
/// notes limit (the old notes stay rather than a cut-off new one).
fn notes_reply(message: &ChatMessage, markers: &[String]) -> Option<String> {
    let text = message.content.trim();
    if text.is_empty()
        || text.eq_ignore_ascii_case("unchanged")
        || message.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty())
        || markers.iter().any(|marker| text.contains(marker.as_str()))
        || text.chars().count() > MAX_NOTES_CHARS
    {
        return None;
    }
    Some(text.to_string())
}

impl Compaction {

    /// Asks the model to rewrite its working notes right before a fold drops the plans from
    /// the history, as a plain reply (no tool call) to one extra user message that is never
    /// stored. The request is the live one — same system prompt, tools and history — so the
    /// model server's cached prompt serves it and only the question is new. A failure or an
    /// unusable reply leaves the notes as they were: the fold still goes ahead, the plan just
    /// isn't saved this time. `think` is the chat's own choice, not a cheaper one: with thinking on the
    /// Qwen template starts the system prompt with a reasoning-effort line, so asking with thinking off
    /// changes the prompt from its first token and the server reuses none of its cache (measured: 0 of
    /// 35k prompt tokens, against all but a few with the same setting).
    pub(super) async fn save_notes_before_fold(&self, chat_id: i64, think: Option<ThinkChoice>, known_prompt_tokens: Option<u64>) {
        let outcome: Result<Option<String>, ErrorService> = async {
            let chat = self.chat_store.chat(chat_id).await?;
            if chat.parent_chat_id.is_some() {
                // A sub-agent's chat ends with its result; nobody reads its notes later
                return Ok(None);
            }
            if !chat.tools_enabled {
                // Notes are part of working with tools; a chat without them is a plain conversation, and the
                // model it runs on is often too small to keep notes worth their cost
                return Ok(None);
            }
            // With thinking on the model reasons before it writes the notes, and a reply that runs out of room
            // mid-reasoning is thrown away after minutes of generation (measured: two requests of 4,096 tokens,
            // 170 s each, no notes). Too little room left in the window: leave the notes to the model's own writes.
            let context = self.model.context_of(&chat).await?;
            if known_prompt_tokens.is_some_and(|known| context.saturating_sub(known) < NOTES_ASK_MIN_ROOM_TOKENS) {
                tracing::info!(chat_id, "notes request skipped: not enough room left in the window to think and write");
                return Ok(None);
            }
            let provider = self.model.provider(&chat)?;
            let params = self.model.params(&chat).await?;
            let tools_snapshot = self.tools.snapshot_tools().await;
            let tools: Vec<&dyn Tool> = tools_snapshot
                .iter()
                .map(|t| t.as_ref())
                .filter(|t| subagent::available_to(t.function_name(), false))
                .collect();
            let mut history = self.history.for_chat(chat_id).await?;
            let system_prompt = self.history.system_prompt(&chat, &mut history).await?;
            let mut messages = vec![ChatMessage::system(system_prompt)];
            messages.extend(history);
            let response = provider
                .chat(messages, Some(ChatMessage::user(prompts::notes_ask())), &tools, think, &chat.model, known_prompt_tokens, &params, None)
                .await?;
            let markers = provider.tool_call_markers(&chat.model).await;
            Ok(notes_reply(&response.message, &markers))
        }
        .await;

        match outcome {
            Ok(Some(notes)) => {
                if let Err(e) = self.chat_store.set_notes(chat_id, Some(notes)).await {
                    tracing::warn!(chat_id, "couldn't save the notes before a fold: {e:?}");
                }
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(
                chat_id,
                "asking for notes before a fold failed: {}",
                e.message.as_deref().unwrap_or("unknown error")
            ),
        }
        // What the model wrote with `chat.write_notes` since the last compaction joins the prompt
        // now: the prompt is rewritten right after this anyway, and the request above has
        // already read the old one (a no-op when that request just replaced the notes)
        if let Err(e) = self.chat_store.apply_pending_notes(chat_id).await {
            tracing::warn!(chat_id, "couldn't apply the pending notes: {e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(content: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_name: None,
            thinking: None,
            images: None,
        }
    }

    #[test]
    fn notes_reply_takes_plain_text_only() {
        let markers = vec!["<tool_call>".to_string()];
        assert_eq!(notes_reply(&reply("  Goal: x\nPlan: y "), &markers).as_deref(), Some("Goal: x\nPlan: y"));
        assert_eq!(notes_reply(&reply("UNCHANGED"), &markers), None);
        assert_eq!(notes_reply(&reply("unchanged\n"), &markers), None);
        assert_eq!(notes_reply(&reply(""), &markers), None);
        assert_eq!(notes_reply(&reply("<tool_call><function=x>"), &markers), None);
        assert_eq!(notes_reply(&reply(&"n".repeat(MAX_NOTES_CHARS + 1)), &markers), None);
    }
}
