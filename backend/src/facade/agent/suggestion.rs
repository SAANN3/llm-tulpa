//! What the user might write next: after a run ends with a reply, one short request asks the chat's model for it and the
//! answer is kept on the chat until the next message.
//!
//! The request is the chat's own prompt (its system message, tools and messages, exactly as a turn sends them) with the question
//! as one more message at the end, so the model server already holds the prompt and only reads the question; it is made
//! with thinking off and a few tokens to spend. A request built without the tools differs from the first token on and
//! makes the server read the whole chat again.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::AbortHandle;

use super::history::History;
use super::model_call::ModelCall;
use super::prompts;
use super::turn::Turn;
use crate::services::chat_store::ChatStore;
use crate::services::error::ErrorService;
use crate::services::event_bus::{EventBus, ServerEvent};
use crate::services::llm::{ChatMessage, ThinkChoice};
use crate::services::settings_store::SettingsStore;
use crate::services::tools::ToolService;
use crate::tools::base::Tool;

/// Tokens the model may spend on the line.
const SUGGESTION_TOKENS: i32 = 48;
/// A request whose prompt the server still holds takes well under a second; one that has to read the chat again (after a fold,
/// or when another chat used the model in between) would hold the GPU for much longer than a suggestion is worth, so it is
/// dropped.
const SUGGESTION_TIMEOUT: Duration = Duration::from_secs(8);
/// The longest line kept.
const SUGGESTION_CHARS: usize = 200;

#[derive(Clone)]
pub(super) struct Suggestion {
    chat_store: Arc<ChatStore>,
    settings_store: Arc<SettingsStore>,
    tools: Arc<ToolService>,
    events: Arc<EventBus>,
    history: History,
    model: ModelCall,
    /// The suggestions being written, one per chat, so a run that starts on the chat drops its own.
    running: Arc<Mutex<HashMap<i64, AbortHandle>>>,
}

impl Suggestion {
    pub(super) fn new(
        chat_store: Arc<ChatStore>,
        settings_store: Arc<SettingsStore>,
        tools: Arc<ToolService>,
        events: Arc<EventBus>,
        history: History,
        model: ModelCall,
    ) -> Self {
        Self { chat_store, settings_store, tools, events, history, model, running: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Starts writing the suggestion for the reply a run has just ended with, in the background.
    pub(super) fn start(&self, chat_id: i64) {
        let this = self.clone();
        let task = tokio::spawn(async move {
            if let Err(e) = this.write(chat_id).await {
                // Best effort: a suggestion is a convenience, and the run it follows has ended
                tracing::debug!(chat_id, "no suggestion: {}", e.message.as_deref().unwrap_or("unknown error"));
            }
            this.running.lock().unwrap().remove(&chat_id);
        });
        if let Some(old) = self.running.lock().unwrap().insert(chat_id, task.abort_handle()) {
            old.abort();
        }
    }

    /// A run starts on the chat: the model is wanted for it, and the line would be stale anyway.
    pub(super) fn cancel(&self, chat_id: i64) {
        if let Some(handle) = self.running.lock().unwrap().remove(&chat_id) {
            handle.abort();
        }
    }

    async fn write(&self, chat_id: i64) -> Result<(), ErrorService> {
        let mut chat = self.chat_store.chat(chat_id).await?;
        // Only an ordinary chat of a user who asked for it: a sub-agent's chat has nobody to write to it, and a messaging
        // plugin's chat is answered from the messaging app
        if chat.parent_chat_id.is_some() || !self.settings_store.suggest_next(chat.user_id).await? || self.chat_store.is_plugin_chat(chat_id).await? {
            return Ok(());
        }
        let Some((reply_id, role, content)) = self.chat_store.newest_message(chat_id).await? else {
            return Ok(());
        };
        if role != "assistant" || content.trim().is_empty() {
            return Ok(());
        }

        self.model.bind(&mut chat);
        let provider = self.model.provider(&chat)?;
        // Asking for a model that isn't loaded would load it, or put another model's launch in its place, for the sake of one line
        // (llama.cpp reports the file name alone, where a registered model may sit in a folder)
        let file = chat.model.rsplit(['/', '\\']).next().unwrap_or(&chat.model);
        let loaded = provider.running_models().await.is_ok_and(|running| running.models.iter().any(|m| m.name == file || m.name == chat.model));
        if !loaded {
            return Ok(());
        }

        let mut messages = self.history.for_chat(chat_id).await?;
        let system_prompt = self.history.system_prompt(&chat, &mut messages).await?;
        let mut request = vec![ChatMessage::system(system_prompt)];
        request.extend(messages);
        request.push(ChatMessage::user(prompts::SUGGEST_NEXT.to_string()));

        let tools = Turn::tools_for(&chat, self.tools.snapshot_tools().await);
        let tools: Vec<&dyn Tool> = tools.iter().map(|t| t.as_ref()).collect();
        let mut params = self.model.params(&chat).await?;
        params.max_reply_tokens = Some(SUGGESTION_TOKENS);

        let call = provider.chat(request, None, &tools, Some(ThinkChoice::Enabled(false)), &chat.model, None, &params, None);
        let response = tokio::time::timeout(SUGGESTION_TIMEOUT, call)
            .await
            .map_err(|_| ErrorService::internal("the model did not answer in time"))??;
        let Some(line) = Self::line_of(&response.message.content) else {
            return Ok(());
        };

        // Said in the meantime: the line is about a chat that has moved on
        if self.chat_store.newest_message(chat_id).await?.map(|(id, _, _)| id) != Some(reply_id) {
            return Ok(());
        }
        self.chat_store.set_suggestion(chat_id, &line, reply_id).await?;
        self.events.publish(ServerEvent::SuggestionReady { chat_id });
        Ok(())
    }

    /// The model's answer as one line to show: the first one with anything in it, without list marks or quotes.
    fn line_of(text: &str) -> Option<String> {
        let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
        let mut line = line.trim_start_matches(|c: char| matches!(c, '-' | '*' | '>')).trim_start();
        // A numbered list's "1." or "2)", not the number a message starts with
        let digits = line.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
        if digits > 0 && matches!(line[digits..].chars().next(), Some('.' | ')')) {
            line = line[digits + 1..].trim_start();
        }
        let line = line.trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '\u{201c}' | '\u{201d}' | '\u{ab}' | '\u{bb}') || c.is_whitespace());
        (line.chars().count() > 1).then(|| line.chars().take(SUGGESTION_CHARS).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_with_text_is_kept_without_marks_or_quotes() {
        assert_eq!(Suggestion::line_of("  \n\"And which one talked about themes?\"\nsecond"), Some("And which one talked about themes?".to_string()));
        assert_eq!(Suggestion::line_of("- 1. fix the test"), Some("fix the test".to_string()));
        assert_eq!(Suggestion::line_of("3 files are left, what now?"), Some("3 files are left, what now?".to_string()));
        assert_eq!(Suggestion::line_of("\u{ab}\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43a}\u{430}\u{43a} \u{434}\u{435}\u{43b}\u{430}?\u{bb}"), Some("\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43a}\u{430}\u{43a} \u{434}\u{435}\u{43b}\u{430}?".to_string()));
    }

    #[test]
    fn nothing_to_show_gives_none() {
        assert_eq!(Suggestion::line_of(""), None);
        assert_eq!(Suggestion::line_of(" \n  "), None);
        assert_eq!(Suggestion::line_of("\"\""), None);
        assert_eq!(Suggestion::line_of("."), None);
    }

    #[test]
    fn a_long_line_is_cut() {
        assert_eq!(Suggestion::line_of(&"x".repeat(500)).unwrap().chars().count(), SUGGESTION_CHARS);
    }
}
