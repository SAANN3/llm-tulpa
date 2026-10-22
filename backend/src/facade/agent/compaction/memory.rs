//! What a fold remembers about the user, for chats to come: the request that asks for it, and what is done with the
//! reply.
//!
//! The request is the live one, like the notes request: the same system prompt, tools and history, and one extra user
//! message that is never stored. The model server's cached prompt serves it and only the question is new, so it costs a
//! short generation and no reading of the chat. It is asked first, before anything that rewrites the prompt (the notes
//! the model saves, the clearing of old tool results, the summary): nothing it does touches this chat's own prompt, the
//! user's memory reaches a prompt only as a snapshot taken when a chat starts or folds.
//!
//! The model is not trusted with the memory. Every entry has to carry a quote of the user's own words that is found in
//! one of the user's messages, so what the assistant or a tool said, or only guessed, is dropped; what is left goes
//! through `MemoryStore::learn`, whose merge is plain code.

use serde::Deserialize;

use super::super::live::StepsReport;
use super::super::prompts;
use super::super::turn::Turn;
use super::{Compaction, MEMORY};
use crate::services::error::ErrorService;
use crate::services::event_bus::{CallKind, StepState};
use crate::services::llm::{ChatMessage, ReplyPiece, ThinkChoice};
use crate::services::memory_store::merge::{normalize, Candidate};
use crate::services::memory_store::{Category, MemoryEntry};
use crate::tools::base::Tool;

/// The reply may be this long, reasoning included: a model that is unsure whether anything is worth keeping can reason for
/// a long time, and nothing it concludes then is worth a fold waiting for. On a 90,000-character chat with five things
/// worth keeping a model reasoned for 1,700 to 4,400 tokens before it answered, so 3,000 cut off most of them.
const MAX_REPLY_TOKENS: i32 = 6_000;
/// The request is only made with at least this many tokens of the window free: the reply may use `MAX_REPLY_TOKENS`, and the
/// question and a margin come on top of the measured prompt. Less than the notes request asks for, since the reply is
/// capped far lower than notes up to 8,000 characters.
const MIN_ROOM_TOKENS: u64 = MAX_REPLY_TOKENS as u64 + 1_000;
/// How much of what is already remembered the question shows the model (it checks its own proposals against it; the merge
/// checks the rest).
const KNOWN_BUDGET_CHARS: usize = 4_000;
/// A quote shorter than this proves nothing.
const MIN_QUOTE_CHARS: usize = 6;

/// What one entry of the reply looks like on the wire.
#[derive(Deserialize)]
struct Proposed {
    category: Option<String>,
    text: Option<String>,
    quote: Option<String>,
    replaces: Option<i64>,
}

#[derive(Deserialize)]
struct Reply {
    memories: Vec<Proposed>,
}

/// What asking for the memory came to
enum Asked {
    /// Not asked, and why
    Not(&'static str),
    Learned(String),
}

/// The entries the question shows, newest first and numbered from 1, within `KNOWN_BUDGET_CHARS`.
fn shown_to_the_model(entries: &[MemoryEntry]) -> Vec<&MemoryEntry> {
    let mut used = 0;
    entries
        .iter()
        .take_while(|entry| {
            used += entry.text.chars().count() + 24;
            used <= KNOWN_BUDGET_CHARS
        })
        .collect()
}

/// What the model answered as proposals for the store: only entries that carry a quote found in `user_text`, with the
/// number of an entry it replaces turned into that entry's id. `None` when the reply is not the JSON that was asked for.
fn proposals(raw: &str, shown: &[&MemoryEntry], user_text: &str) -> Option<Vec<Candidate>> {
    let cleaned = Compaction::strip_json_fences(raw);
    let reply: Reply = serde_json::from_str(cleaned).ok()?;
    let user_text = normalize(user_text);
    Some(
        reply
            .memories
            .into_iter()
            .filter_map(|proposed| {
                let text = proposed.text?;
                let quote = proposed.quote?;
                let wanted = normalize(&quote);
                if wanted.chars().count() < MIN_QUOTE_CHARS || !user_text.contains(&wanted) {
                    return None;
                }
                Some(Candidate {
                    category: proposed.category.as_deref().and_then(Category::parse).unwrap_or(Category::AboutYou),
                    text,
                    quote: quote.split_whitespace().collect::<Vec<_>>().join(" "),
                    replaces: proposed
                        .replaces
                        .and_then(|number| usize::try_from(number).ok())
                        .and_then(|number| number.checked_sub(1))
                        .and_then(|at| shown.get(at))
                        .map(|entry| entry.id),
                })
            })
            .collect(),
    )
}

impl Compaction {
    /// Asks the model what is worth remembering about the user from this chat and stores it. Best-effort: a failure, a reply
    /// that is not what was asked for, or nothing worth keeping leaves the memory as it was, and the fold goes on.
    pub(super) async fn save_memory_before_fold(
        &self,
        chat_id: i64,
        think: Option<ThinkChoice>,
        known_prompt_tokens: Option<u64>,
        report: Option<&StepsReport>,
    ) {
        let outcome: Result<Asked, ErrorService> = async {
            let Some(memory_store) = &self.memory_store else {
                return Ok(Asked::Not("not asked in a messaging plugin's chat"));
            };
            let chat = self.chat_store.chat(chat_id).await?;
            if chat.parent_chat_id.is_some() {
                // A sub-agent works for the chat that handed it a task; what it learns about the user is that chat's business
                return Ok(Asked::Not("a sub-agent's chat"));
            }
            let prefs = self.settings_store.memory_prefs(chat.user_id).await?;
            if !prefs.enabled {
                return Ok(Asked::Not("memory is off"));
            }
            // A reply that runs out of room mid-reasoning is minutes of generation for nothing
            let context = self.model.context_of(&chat).await?;
            if known_prompt_tokens.is_some_and(|known| context.saturating_sub(known) < MIN_ROOM_TOKENS) {
                return Ok(Asked::Not("not enough room left in the window"));
            }
            let provider = self.model.provider(&chat)?;
            let mut params = self.model.params(&chat).await?;
            params.max_reply_tokens = Some(MAX_REPLY_TOKENS);
            // The tools the live request sends, whatever the chat: the prompt has to be the same one the server holds
            let tools_snapshot = Turn::tools_for(&chat, self.tools.snapshot_tools().await);
            let tools: Vec<&dyn Tool> = tools_snapshot.iter().map(|t| t.as_ref()).collect();
            let mut history = self.history.for_chat(chat_id).await?;
            let system_prompt = self.history.system_prompt(&chat, &mut history).await?;
            let user_text = history.iter().filter(|message| message.role == "user").map(|m| m.content.as_str()).collect::<Vec<_>>().join("\n");
            let mut messages = vec![ChatMessage::system(system_prompt)];
            messages.extend(history);

            let entries = memory_store.list(chat.user_id).await?;
            let shown = shown_to_the_model(&entries);
            let known: String = shown
                .iter()
                .enumerate()
                .map(|(at, entry)| format!("{}. [{}] {}", at + 1, entry.category.as_str(), entry.text))
                .collect::<Vec<_>>()
                .join("\n");

            let call = report.map(|report| report.call(&MEMORY, CallKind::Memory));
            let on_piece = |piece: ReplyPiece| {
                if let Some(call) = &call {
                    call.piece(&piece);
                }
            };
            let response = provider
                .chat(messages, Some(ChatMessage::user(prompts::memory_ask(&known))), &tools, think, &chat.model, known_prompt_tokens, &params, Some(&on_piece))
                .await?;
            let Some(candidates) = proposals(&response.message.content, &shown, &user_text) else {
                tracing::info!(chat_id, "the memory reply was not usable");
                return Ok(Asked::Not("the reply could not be used"));
            };
            if candidates.is_empty() {
                return Ok(Asked::Learned("nothing worth remembering".to_string()));
            }
            let learned = memory_store.learn(chat.user_id, chat_id, candidates, prefs.max_entries).await?;
            Ok(Asked::Learned(format!(
                "{} added, {} corrected, {} left out{}",
                learned.added,
                learned.replaced,
                learned.skipped,
                if learned.dropped > 0 { format!(", {} old ones dropped for the limit", learned.dropped) } else { String::new() }
            )))
        }
        .await;

        if let Some(report) = report {
            match &outcome {
                Ok(Asked::Not(why)) => report.set(&MEMORY, StepState::Skipped, Some(why)),
                Ok(Asked::Learned(what)) => report.set(&MEMORY, StepState::Done, Some(what)),
                Err(e) => report.set(&MEMORY, StepState::Failed, Some(e.message.as_deref().unwrap_or("the request failed"))),
            }
        }
        if let Err(e) = outcome {
            tracing::warn!(chat_id, "asking for the memory before a fold failed: {}", e.message.as_deref().unwrap_or("unknown error"));
        }
    }

    /// What a fold asks of the model while its prompt is still the one the server holds: first what to remember about the
    /// user, then the notes. Neither changes the prompt the other is asked on.
    pub(super) async fn ask_before_fold(&self, chat_id: i64, think: Option<ThinkChoice>, known_prompt_tokens: Option<u64>, report: Option<&StepsReport>) {
        self.save_memory_before_fold(chat_id, think.clone(), known_prompt_tokens, report).await;
        self.save_notes_before_fold(chat_id, think, known_prompt_tokens, report).await;
    }

    /// Takes the user's memory, as it is now, into the chat's prompt: a fold rewrites the prompt anyway. Best-effort.
    pub(super) async fn refresh_memory_after_fold(&self, chat_id: i64) {
        let refreshed = async {
            let mut chat = self.chat_store.chat(chat_id).await?;
            self.history.refresh_memory(&mut chat).await
        }
        .await;
        if let Err(e) = refreshed {
            tracing::warn!(chat_id, "couldn't take the user's memory into the chat after a fold: {}", e.message.as_deref().unwrap_or("unknown error"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn entry(id: i64, text: &str) -> MemoryEntry {
        MemoryEntry { id, category: Category::Misc, text: text.to_string(), quote: None, learned: true, source_chat_id: None, updated_at: Utc::now() }
    }

    #[test]
    fn only_entries_with_a_quote_found_in_the_users_messages_are_proposed() {
        let reply = r#"```json
        {"memories":[
          {"category":"preferences","text":"Answers in Russian.","quote":"please always answer me in Russian","replaces":null},
          {"category":"about_you","text":"Uses Linux.","quote":"I use Linux daily","replaces":null},
          {"category":"nonsense","text":"Has a cat.","quote":"my   CAT is called Tom","replaces":null},
          {"category":"misc","text":"short quote","quote":"hi","replaces":null},
          {"category":"misc","text":"no quote at all"}
        ]}
        ```"#;
        let user_text = "Hello. Please always answer me in Russian, thanks.\nAlso my cat is called Tom.";
        let found = proposals(reply, &[], user_text).unwrap();
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].category, Category::Preferences);
        // An unknown category is read as about_you, and a quote matches whatever its spacing and case
        assert_eq!(found[1].category, Category::AboutYou);
        assert_eq!(found[1].quote, "my CAT is called Tom");
    }

    #[test]
    fn a_number_the_model_replaces_is_turned_into_the_id_of_the_entry_it_saw() {
        let entries = [entry(41, "first"), entry(42, "second")];
        let shown: Vec<&MemoryEntry> = entries.iter().collect();
        let reply = r#"{"memories":[{"category":"misc","text":"changed","quote":"I changed my mind","replaces":2},
                                   {"category":"misc","text":"other","quote":"I changed my mind","replaces":9}]}"#;
        let found = proposals(reply, &shown, "I changed my mind").unwrap();
        assert_eq!(found[0].replaces, Some(42));
        assert_eq!(found[1].replaces, None);
    }

    #[test]
    fn a_reply_that_is_not_the_asked_for_json_is_not_a_proposal() {
        assert!(proposals("Here is some text, not JSON", &[], "x").is_none());
        assert!(proposals("", &[], "x").is_none());
        assert_eq!(proposals(r#"{"memories":[]}"#, &[], "x").unwrap().len(), 0);
    }

    #[test]
    fn the_question_shows_the_newest_entries_up_to_its_budget() {
        let entries: Vec<MemoryEntry> = (0..100).map(|i| entry(i, &"x".repeat(300))).collect();
        let shown = shown_to_the_model(&entries);
        assert!(!shown.is_empty() && shown.len() < entries.len());
        assert_eq!(shown[0].id, 0, "the list is already newest first");
    }
}
