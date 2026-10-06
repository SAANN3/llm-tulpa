//! What the model is sent for a chat: the system message (the user's own system prompt or the built-in
//! one, the sub-agent addendum, and after a compaction the key facts, the user's pinned messages, the
//! summary and the model's notes) and the messages after it, with old tool results as stubs and
//! thinking replayed at its cap.
//!
//! Loading rows and building from them are separate steps: `load` reads the stores, `build` is a pure
//! function of what was loaded. The bytes it produces are what the model server's cached prompt is
//! matched against, so they must not change from one request to the next unless a compaction moved
//! a boundary; the tests here pin them.

use std::sync::Arc;

use serde_json::Value;

use super::compaction::clearing;
use super::prompts;
use crate::services::chat_store::{Chat, ChatStore, Message};
use crate::services::error::ErrorService;
use crate::services::llm::{ChatMessage, ModelToolCall, ModelToolCallFunction};
use crate::services::settings_store::SettingsStore;
use crate::tools::base::Tool;
use crate::tools::ui::attach_file::AttachFileTool;

mod pinned;

/// Maximum number of characters of reasoning (`message.thinking`) preserved when
/// replaying an assistant message back to the model in `to_ollama_message`.
///
/// Qwen 3.8 and similar models produce rich reasoning traces that are critical for
/// avoiding amnesia and repetitive exploration loops across turns. However, an
/// anomalous run-away reasoning turn could single-handedly consume the uncompacted
/// history budget (`keep_chars`). Capping at 10,000 characters (~2,500–3,000
/// tokens) preserves several turns of deep reasoning while preventing pathological
/// budget exhaustion.
const MAX_REPLAYED_THINKING_CHARS: usize = 10_000;

/// Threshold of consecutive read-only tool calls without editing or writing files
/// before injecting a dynamic circuit-breaker notice into the prompt.
const READ_ONLY_STREAK_THRESHOLD: usize = 5;

#[derive(Clone)]
pub(super) struct History {
    chat_store: Arc<ChatStore>,
    settings_store: Arc<SettingsStore>,
}

/// Everything `History::build` needs, as loaded from the stores.
struct Inputs {
    chat: Chat,
    /// The user's setting: old thinking is replayed at the cap its place relative to the stored boundary gives
    trim_thinking: bool,
    /// What the prompt carries: the messages after the compaction boundary, or the whole chat before the first fold
    messages: Vec<Message>,
    /// The user's own messages the summary replaced, for the pinned block (empty before the first fold)
    folded_user_texts: Vec<(i64, String)>,
}

impl History {
    pub(super) fn new(chat_store: Arc<ChatStore>, settings_store: Arc<SettingsStore>) -> Self {
        Self { chat_store, settings_store }
    }

    /// A chat's message history mapped into Ollama's wire format, oldest first. Once a
    /// chat has a compaction summary (`Chat::summary`/`summary_up_to_message_id` — see
    /// `compact`), that replaces everything up to the boundary as a single system
    /// message, prefixed with key facts (if any). Only what's newer is sent verbatim;
    /// otherwise this is the whole history, same as before compaction existed.
    pub(super) async fn for_chat(&self, chat_id: i64) -> Result<Vec<ChatMessage>, ErrorService> {
        Ok(Self::build(self.load(chat_id).await?))
    }

    async fn load(&self, chat_id: i64) -> Result<Inputs, ErrorService> {
        let chat = self.chat_store.chat(chat_id).await?;
        let trim_thinking = self.settings_store.trim_old_thinking(chat.user_id).await?;
        let (messages, folded_user_texts) = match (&chat.summary, chat.summary_up_to_message_id) {
            (Some(_), Some(boundary_id)) => (
                self.chat_store.messages_after(chat_id, boundary_id).await?,
                self.chat_store.user_texts_up_to(chat_id, boundary_id).await?,
            ),
            _ => (self.chat_store.messages_after(chat_id, 0).await?, Vec::new()),
        };
        Ok(Inputs { chat, trim_thinking, messages, folded_user_texts })
    }

    fn build(inputs: Inputs) -> Vec<ChatMessage> {
        let Inputs { chat, trim_thinking, mut messages, folded_user_texts } = inputs;
        clearing::stub_cleared(&mut messages, chat.cleared_up_to_message_id);
        // With old thinking trimmed (the user's choice), each trace is replayed at the cap its
        // place relative to the stored boundary gives; otherwise every trace at the default cap
        let to_model = |message: Message| {
            if trim_thinking {
                let cap = clearing::thinking_cap(message.id, chat.thinking_trimmed_up_to_message_id);
                Self::to_message_capped(message, cap)
            } else {
                Self::to_message(message)
            }
        };
        let notes = Self::notes_block(chat.notes.as_deref());

        match (&chat.summary, chat.summary_up_to_message_id) {
            (Some(summary), Some(_)) => {
                let mut system_content = String::from(prompts::fold_header(chat.tools_enabled));

                // Prepend key facts (goal + list) if available. Facts are durable —
                // they persist across folds and don't get rewritten.
                if let Some(ref key_facts) = chat.key_facts {
                    system_content.push_str("\n\nKey facts (durable; still in effect unless a later message contradicts them):");
                    if let Some(ref goal) = key_facts.goal {
                        system_content.push_str(&format!("\nGoal: {goal}"));
                    }
                    for fact in &key_facts.facts {
                        system_content.push_str(&format!("\n- {fact}"));
                    }
                }

                if let Some(pinned) = pinned::section(&folded_user_texts, chat.tools_enabled) {
                    system_content.push_str("\n\n");
                    system_content.push_str(&pinned);
                }

                system_content.push_str("\n\nSummary of everything before this point:\n\n");
                system_content.push_str(summary);
                if let Some(notes) = notes {
                    system_content.push_str("\n\n");
                    system_content.push_str(&notes);
                }

                let mut history = vec![ChatMessage::system(system_content)];
                history.extend(messages.into_iter().map(to_model));
                history
            }
            _ => {
                let mut history: Vec<ChatMessage> = messages.into_iter().map(to_model).collect();
                // No summary yet, but notes written early still have to reach the model
                if let Some(notes) = notes {
                    history.insert(0, ChatMessage::system(notes));
                }
                history
            }
        }
    }

    /// The model's notes as the block that goes after the summary in the system message, or `None`
    /// when it has written none.
    fn notes_block(notes: Option<&str>) -> Option<String> {
        let notes = notes?.trim();
        (!notes.is_empty()).then(|| format!("{}\n\n{notes}", prompts::NOTES_HEADER))
    }

    /// The one system message a request to the model starts with: the user's own system prompt
    /// (or the built-in one), the sub-agent addendum for a sub-agent's chat, and, when the history
    /// leads with one, that message (the compaction summary and notes) — taken out of `messages`.
    /// Shared by every request built for a chat so they all share the same prefix.
    pub(super) async fn system_prompt(&self, chat: &Chat, messages: &mut Vec<ChatMessage>) -> Result<String, ErrorService> {
        let custom = self.settings_store.system_prompt(chat.user_id).await?;
        let leading = messages.first().is_some_and(|message| message.role == "system").then(|| messages.remove(0).content);
        Ok(Self::compose_system_prompt(custom, chat.tools_enabled, chat.parent_chat_id.is_some(), leading))
    }

    /// The user's own prompt is sent as written, tools or not: only the built-in one, which is ours, has a
    /// version for a chat without tools.
    fn compose_system_prompt(custom: Option<String>, tools: bool, is_subagent: bool, leading: Option<String>) -> String {
        let mut system_prompt = custom.unwrap_or_else(|| if tools { prompts::default_system_prompt() } else { prompts::default_system_prompt_without_tools() });
        if is_subagent {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&prompts::subagent_system_prompt());
        }
        if let Some(leading) = leading {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&leading);
        }
        system_prompt
    }

    /// Truncates reasoning trace to at most `MAX_REPLAYED_THINKING_CHARS`, keeping the
    /// *tail* (most recent reasoning) rather than the head: final conclusions, plan
    /// adjustments, and next-step decisions are reached toward the end of a thought block.
    ///
    /// Ensures strict UTF-8 char boundary safety, aligns to a newline boundary where
    /// reasonable to avoid splitting mid-word, and prepends a clear truncation notice so
    /// the model understands it is viewing the tail of its previous thoughts.
    fn cap_thinking(thinking: &str, max_chars: usize) -> String {
        if thinking.len() <= max_chars {
            return thinking.to_string();
        }

        let mut start = thinking.len() - max_chars;
        while start < thinking.len() && !thinking.is_char_boundary(start) {
            start += 1;
        }

        // If there is a newline within the first 500 characters after the raw cut,
        // advance past it to start on a clean line of reasoning rather than mid-sentence.
        let clean_start = thinking[start..]
            .find('\n')
            .map(|idx| start + idx + 1)
            .filter(|&idx| idx - start <= 500 && idx < thinking.len())
            .unwrap_or(start);

        format!("... [earlier thinking truncated] ...\n{}", &thinking[clean_start..])
    }

    pub(super) fn streak_notice(messages: &[ChatMessage]) -> Option<String> {
        let mut streak = 0;
        for msg in messages.iter().rev() {
            if msg.role == "user" {
                break;
            }
            if let Some(ref name) = msg.tool_name {
                match name.as_str() {
                    "storage.write_file" | "storage.replace_str" | "storage.delete_file" => break,
                    "storage.read_file" | "storage.list_directory" | "storage.detect_file_type" => {
                        streak += 1;
                    }
                    _ => {}
                }
            } else if let Some(ref calls) = msg.tool_calls {
                let has_write = calls.iter().any(|c| {
                    matches!(
                        c.function.name.as_str(),
                        "storage.write_file" | "storage.replace_str" | "storage.delete_file"
                    )
                });
                if has_write {
                    break;
                }
            }
        }

        if streak >= READ_ONLY_STREAK_THRESHOLD {
            tracing::info!(streak, "read-only tool streak threshold reached, injecting circuit breaker notice");
            Some(format!(
                "\n\n[Notice: You have made {streak} consecutive read-only tool calls without editing \
                 or writing any files. If you already know which file(s) to change and what the code \
                 should do, stop reading and make the edit now. If you're trying to verify an API \
                 signature or external interface before writing code, write your best-guess implementation \
                 and run the project's native build, compiler, type-checker, or test tool to verify it. \
                 If there is a genuinely essential piece of information you still need, state it \
                 specifically before your next tool call.]"
            ))
        } else {
            None
        }
    }

    /// Scans back through this turn's already-loaded history for `ui.attach_file`
    /// results, collecting their `file_id`s. Stops at the last `user`-role message
    /// (or the start of history), since that's where the current turn began — any
    /// `tool`/`assistant` messages before it belong to an earlier turn, whose own
    /// attach results were already attached to *that* turn's final reply when it was
    /// generated. Everything from here to the end of history is this turn's own
    /// tool-calling rounds (`assistant` messages requesting tools, `tool` messages
    /// with their results), so no other role needs to stop the scan.
    pub(super) fn attached_files(messages: &[ChatMessage]) -> Vec<i64> {
        let attach_file_name = AttachFileTool.function_name();
        let mut file_ids: Vec<i64> = messages
            .iter()
            .rev()
            .take_while(|message| message.role != "user")
            .filter(|message| message.role == "tool" && message.tool_name.as_deref() == Some(attach_file_name))
            .filter_map(|message| serde_json::from_str::<Value>(&message.content).ok())
            .filter_map(|value| value.get("file_id").and_then(Value::as_i64))
            .collect();
        file_ids.reverse();
        file_ids
    }

    /// Maps a persisted `Message` back into the shape Ollama's `/api/chat` expects for
    /// history. `ModelToolCall::id` is left empty — we never persisted Ollama's
    /// original per-call id (only `function.name`/`arguments`, which is all replaying
    /// history needs), and it's not yet confirmed whether Ollama expects/uses `id` at
    /// all on the *outgoing* (request) side versus just returning it in responses.
    ///
    /// Preserving reasoning context across turns:
    /// For each message with non-empty `thinking`, the reasoning trace is prepended to
    /// `content` wrapped in `<think>\n{thinking}\n</think>\n\n{content}`. The wrapping
    /// happens AFTER `with_attached_files_note` so any file annotations remain inside
    /// `content` under the `<think>` block.
    ///
    /// We deliberately keep `ChatMessage::thinking` as `None` rather than passing a
    /// separate field: upstream chat templates and API proxies (such as OpenAI-compatible
    /// endpoints like `llama-mtp`) do not consistently support or forward a separate
    /// reasoning input field on prior turns, whereas textual concatenation inside `content` is
    /// universally rendered by all chat templates and models.
    ///
    /// Replayed thinking is capped at `MAX_REPLAYED_THINKING_CHARS` (keeping the freshest tail)
    /// to prevent an anomalous reasoning turn from consuming the uncompacted context budget.
    fn to_message(message: Message) -> ChatMessage {
        Self::to_message_capped(message, MAX_REPLAYED_THINKING_CHARS)
    }

    /// `to_ollama_message` with the replayed thinking capped at `thinking_cap` characters instead of the default.
    fn to_message_capped(message: Message, thinking_cap: usize) -> ChatMessage {
        let tool_calls: Vec<ModelToolCall> = message
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(index, call)| ModelToolCall {
                id: String::new(),
                function: ModelToolCallFunction {
                    index: Some(index as u32),
                    name: call.tool_name,
                    arguments: call.arguments,
                },
            })
            .collect();

        let mut content = prompts::with_attached_files_note(message.content, &message.file_ids);
        if let Some(thinking) = message.thinking.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            let capped = Self::cap_thinking(thinking, thinking_cap);
            content = format!("<think>\n{capped}\n</think>\n\n{content}");
        }

        ChatMessage {
            // A `notice` is the backend telling the model something (a job finished) —
            // chat templates only know system/user/assistant/tool, and it reads as
            // something said to the model, so it goes out as a `user` message.
            role: if message.role == "notice" { "user".to_string() } else { message.role },
            content,
            tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            tool_name: message.tool_name,
            thinking: None,
            images: (!message.images.is_empty()).then_some(message.images),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::chat_store::{ChatFacts, ToolCallOut};
    use chrono::Utc;
    use serde_json::json;

    fn make_test_message(role: &str, content: &str, thinking: Option<&str>, file_ids: Vec<i64>) -> Message {
        Message {
            id: 1,
            chat_id: 1,
            role: role.to_string(),
            content: content.to_string(),
            tool_name: None,
            created_at: Utc::now(),
            thinking: thinking.map(String::from),
            thought_duration_ms: None,
            tool_success: None,
            tool_denied: false,
            tool_calls: vec![],
            images: vec![],
            file_ids,
            prompt_tokens: None,
            eval_tokens: None,
        }
    }

    #[test]
    fn test_to_message_without_thinking() {
        let msg = make_test_message("assistant", "Hello world", None, vec![]);
        let ollama_msg = History::to_message(msg);
        assert_eq!(ollama_msg.role, "assistant");
        assert_eq!(ollama_msg.content, "Hello world");
        assert!(ollama_msg.thinking.is_none());
    }

    #[test]
    fn test_to_message_with_thinking() {
        let msg = make_test_message(
            "assistant",
            "Here is the plan.",
            Some("Let me reason step by step.\n1. Inspect codebase.\n2. Fix issue."),
            vec![],
        );
        let ollama_msg = History::to_message(msg);
        assert_eq!(ollama_msg.role, "assistant");
        assert_eq!(
            ollama_msg.content,
            "<think>\nLet me reason step by step.\n1. Inspect codebase.\n2. Fix issue.\n</think>\n\nHere is the plan."
        );
        assert!(ollama_msg.thinking.is_none());
    }

    #[test]
    fn test_to_message_with_thinking_and_empty_content() {
        let mut msg = make_test_message("assistant", "", Some("Deciding which tool to call..."), vec![]);
        msg.tool_calls.push(ToolCallOut {
            tool_name: "storage.read_file".to_string(),
            arguments: json!({"path": "src/main.rs"}),
        });
        let ollama_msg = History::to_message(msg);
        assert_eq!(ollama_msg.role, "assistant");
        assert_eq!(
            ollama_msg.content,
            "<think>\nDeciding which tool to call...\n</think>\n\n"
        );
        assert!(ollama_msg.thinking.is_none());
        assert!(ollama_msg.tool_calls.is_some());
    }

    #[test]
    fn test_to_message_with_attached_files_and_thinking() {
        let msg = make_test_message(
            "assistant",
            "Checking attachments",
            Some("I see the user mentioned an attachment."),
            vec![42, 99],
        );
        let ollama_msg = History::to_message(msg);
        assert!(ollama_msg.content.starts_with("<think>\nI see the user mentioned an attachment.\n</think>\n\n[This message has file(s) attached: id 42, id 99."));
        assert!(ollama_msg.content.ends_with("]\nChecking attachments"));
        assert!(ollama_msg.thinking.is_none());
    }

    #[test]
    fn test_to_message_whitespace_thinking_ignored() {
        let msg = make_test_message("assistant", "No real thinking here", Some("   \n\t  "), vec![]);
        let ollama_msg = History::to_message(msg);
        assert_eq!(ollama_msg.content, "No real thinking here");
        assert!(ollama_msg.thinking.is_none());
    }

    #[test]
    fn test_to_message_notice_role_mapped_to_user() {
        let msg = make_test_message("notice", "Job finished: output 42", None, vec![]);
        let ollama_msg = History::to_message(msg);
        assert_eq!(ollama_msg.role, "user");
        assert_eq!(ollama_msg.content, "Job finished: output 42");
    }

    #[test]
    fn test_cap_thinking_short() {
        let short = "Step 1: Check tests.\nStep 2: Done.";
        assert_eq!(History::cap_thinking(short, MAX_REPLAYED_THINKING_CHARS), short);
    }

    #[test]
    fn test_cap_thinking_truncation() {
        let line = "Thinking iteration about complex logic...\n";
        let repeat_count = (MAX_REPLAYED_THINKING_CHARS / line.len()) + 50;
        let mut long_thinking = String::new();
        for i in 0..repeat_count {
            long_thinking.push_str(&format!("{i}: {line}"));
        }
        long_thinking.push_str("FINAL CONCLUSION: Solution reached.");

        let capped = History::cap_thinking(&long_thinking, MAX_REPLAYED_THINKING_CHARS);
        assert!(capped.starts_with("... [earlier thinking truncated] ...\n"));
        assert!(capped.ends_with("FINAL CONCLUSION: Solution reached."));
        assert!(capped.len() <= MAX_REPLAYED_THINKING_CHARS + 100);
    }

    #[test]
    fn test_cap_thinking_utf8_safety() {
        let cyrillic_thought = "Проверяем многобайтовые символы Юникода для безопасности границ среза.\n";
        let repeat_count = (MAX_REPLAYED_THINKING_CHARS / cyrillic_thought.len()) + 50;
        let mut long_thinking = String::new();
        for _ in 0..repeat_count {
            long_thinking.push_str(cyrillic_thought);
        }
        long_thinking.push_str("Финальный вывод: тест пройден успешно.");

        let capped = History::cap_thinking(&long_thinking, MAX_REPLAYED_THINKING_CHARS);
        assert!(capped.starts_with("... [earlier thinking truncated] ...\n"));
        assert!(capped.ends_with("Финальный вывод: тест пройден успешно."));
    }


    fn chat() -> Chat {
        Chat {
            id: 1,
            user_id: 1,
            name: "c".into(),
            model_id: 1,
            provider: "llama_cpp".into(),
            model: "m".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            summary: None,
            summary_up_to_message_id: None,
            key_facts: None,
            last_prompt_tokens: None,
            folder_id: None,
            parent_chat_id: None,
            launch_profile_id: None,
            notes: None,
            notes_pending: None,
            cleared_up_to_message_id: None,
            thinking_trimmed_up_to_message_id: None,
            tools_enabled: true,
        }
    }

    fn message(id: i64, role: &str, content: &str) -> Message {
        let mut m = make_test_message(role, content, None, vec![]);
        m.id = id;
        m
    }

    fn inputs(chat: Chat, messages: Vec<Message>) -> Inputs {
        Inputs { chat, trim_thinking: false, messages, folded_user_texts: vec![] }
    }

    #[test]
    fn without_a_summary_the_history_is_the_messages_as_they_are() {
        let history = History::build(inputs(chat(), vec![message(1, "user", "hi"), message(2, "assistant", "hello")]));
        assert_eq!(history.iter().map(|m| (m.role.as_str(), m.content.as_str())).collect::<Vec<_>>(), [("user", "hi"), ("assistant", "hello")]);
    }

    #[test]
    fn notes_written_before_the_first_fold_lead_the_history() {
        let mut c = chat();
        c.notes = Some(" plan: a ".into());
        let history = History::build(inputs(c, vec![message(1, "user", "hi")]));
        assert_eq!(history[0].role, "system");
        assert_eq!(history[0].content, format!("{}\n\nplan: a", prompts::NOTES_HEADER));
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn after_a_fold_the_system_message_is_header_facts_pinned_summary_and_notes_in_that_order() {
        let mut c = chat();
        c.summary = Some("1. ESTABLISHED FACTS & FINDINGS: x".into());
        c.summary_up_to_message_id = Some(10);
        c.key_facts = Some(ChatFacts { goal: Some("ship it".into()), facts: vec!["port 4711".into(), "host otter".into()] });
        c.notes = Some("plan: a".into());
        let mut i = inputs(c, vec![message(11, "user", "next")]);
        i.folded_user_texts = vec![(3, "first ask".into()), (7, "second ask".into())];
        let history = History::build(i);
        let expected = format!(
            "{}\n\nKey facts (durable; still in effect unless a later message contradicts them):\nGoal: ship it\n- port 4711\n- host otter\n\n{}- first ask\n- second ask\n\nSummary of everything before this point:\n\n1. ESTABLISHED FACTS & FINDINGS: x\n\n{}\n\nplan: a",
            prompts::FOLD_HEADER,
            prompts::PINNED_HEADER,
            prompts::NOTES_HEADER
        );
        assert_eq!(history[0].role, "system");
        assert_eq!(history[0].content, expected);
        assert_eq!(history[1].content, "next");
    }

    #[test]
    fn building_twice_gives_the_same_bytes() {
        let mut c = chat();
        c.summary = Some("s".into());
        c.summary_up_to_message_id = Some(1);
        c.cleared_up_to_message_id = Some(3);
        let build = || {
            let mut tool = message(3, "tool", &"r".repeat(2_000));
            tool.tool_name = Some("storage.read_file".into());
            let mut i = inputs(c_clone(&c), vec![message(2, "user", "q"), tool, message(4, "assistant", "a")]);
            i.folded_user_texts = vec![(1, "old".into())];
            History::build(i).iter().map(|m| format!("{}|{}", m.role, m.content)).collect::<Vec<_>>()
        };
        assert_eq!(build(), build());
    }

    fn c_clone(c: &Chat) -> Chat {
        Chat { summary: c.summary.clone(), cleared_up_to_message_id: c.cleared_up_to_message_id, summary_up_to_message_id: c.summary_up_to_message_id, ..chat() }
    }

    #[test]
    fn results_up_to_the_boundary_go_out_as_stubs_and_later_ones_in_full() {
        let mut c = chat();
        c.cleared_up_to_message_id = Some(2);
        let mut old = message(2, "tool", &"o".repeat(2_000));
        old.tool_name = Some("storage.read_file".into());
        let mut new = message(4, "tool", &"n".repeat(2_000));
        new.tool_name = Some("storage.read_file".into());
        let history = History::build(inputs(c, vec![message(1, "assistant", "calling"), old, message(3, "assistant", "again"), new]));
        assert!(history[1].content.starts_with("[Result cleared to save context: storage.read_file("), "{}", history[1].content);
        assert_eq!(history[3].content.len(), 2_000);
    }

    #[test]
    fn thinking_is_replayed_at_the_default_cap_or_at_the_cap_its_place_gives() {
        let long = "t".repeat(30_000);
        let mut m = message(5, "assistant", "answer");
        m.thinking = Some(long.clone());
        let mut default_cap = inputs(chat(), vec![m]);
        let capped = History::build(Inputs { messages: default_cap.messages.drain(..).collect(), ..default_cap });
        assert!(capped[0].content.len() < 10_200 && capped[0].content.contains("[earlier thinking truncated]"));

        let mut c = chat();
        c.thinking_trimmed_up_to_message_id = Some(4);
        let mut m = message(5, "assistant", "answer");
        m.thinking = Some(long);
        let trimmed_on = History::build(Inputs { chat: c, trim_thinking: true, messages: vec![m], folded_user_texts: vec![] });
        // message 5 is past the boundary 4: replayed at the larger fresh cap
        assert!(trimmed_on[0].content.len() > 20_000 && trimmed_on[0].content.len() < 24_300);
    }

    #[test]
    fn the_system_prompt_is_the_users_or_the_built_in_one_then_the_sub_agent_addendum_then_the_summary() {
        let plain = History::compose_system_prompt(None, true, false, None);
        assert_eq!(plain, prompts::default_system_prompt());
        let custom = History::compose_system_prompt(Some("mine".into()), true, true, Some("SUMMARY".into()));
        assert_eq!(custom, format!("mine\n\n{}\n\nSUMMARY", prompts::subagent_system_prompt()));
    }

    #[test]
    fn notes_are_absent_without_text() {
        assert_eq!(History::notes_block(None), None);
        assert_eq!(History::notes_block(Some("  \n")), None);
        assert!(History::notes_block(Some("plan: a")).unwrap().ends_with("plan: a"));
    }

    #[test]
    fn a_call_in_a_message_keeps_its_name_and_arguments() {
        let mut m = message(1, "assistant", "");
        m.tool_calls = vec![ToolCallOut { tool_name: "storage.read_file".into(), arguments: json!({"path": "/a"}) }];
        let out = History::to_message(m);
        let call = &out.tool_calls.unwrap()[0];
        assert_eq!((call.function.name.as_str(), call.function.arguments.to_string().as_str()), ("storage.read_file", r#"{"path":"/a"}"#));
    }
}
