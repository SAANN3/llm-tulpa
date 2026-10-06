//! Clearing old tool results: once the oldest ones outweigh a token budget, they go out as a
//! one-line stub (which tool, with what arguments, how big, how it began) instead of in full.
//! Tool results are most of a tool-heavy chat (a file read is ~10k tokens) and the model has
//! already used the old ones; the stub says how to get one back. The same mechanism shortens old
//! thinking traces to their tail, for users who chose that, and lets the newest ones through longer.
//!
//! The logic is pure functions over messages; `Compaction::plan_clearing` is the one way in that reads the
//! chat and uses them. Each boundary is stored on the chat (`cleared_up_to_message_id`,
//! `thinking_trimmed_up_to_message_id`) and only moves forward in one batch, when the context
//! passes the compaction trigger: clearing a little every turn would rewrite the prompt from the
//! first cleared message on, every turn, and the model server's cached prefix would never hold.
//! A stub depends only on the message it replaces, so the same boundary always renders the same bytes.

use std::collections::VecDeque;

use serde_json::Value;

use super::Compaction;
use crate::services::chat_store::{Message, ToolCallOut};
use crate::services::error::ErrorService;

/// After old tool results were cleared at the trigger, no fold is needed when the prompt is
/// estimated to be under this fraction of the window (see `Compaction::maybe_compact`).
pub(super) const CLEARED_ENOUGH_FRACTION: f64 = 0.60;

/// What one clearing pass would change: the new boundaries (`None` leaves one where it is) and the
/// characters each takes out of the prompt.
pub(super) struct ClearPlan {
    pub(super) cleared_up_to: Option<i64>,
    pub(super) thinking_trimmed_up_to: Option<i64>,
    freed_results: usize,
    freed_thinking: usize,
}

impl ClearPlan {
    pub(super) fn freed_chars(&self) -> usize {
        self.freed_results + self.freed_thinking
    }
}

impl Compaction {
    /// Where the chat's cleared-results and trimmed-thinking boundaries would move, without moving
    /// them: `None` when there is nothing to clear.
    pub(super) async fn plan_clearing(&self, chat_id: i64, context: u64) -> Result<Option<ClearPlan>, ErrorService> {
        let chat = self.chat_store.chat(chat_id).await?;
        let messages = self
            .chat_store
            .messages_after(chat_id, chat.summary_up_to_message_id.unwrap_or(0))
            .await?;
        let results = pick_boundary(&messages, chat.cleared_up_to_message_id, fresh_budget_chars(context));
        // Old thinking is only touched when the user chose that
        let thinking = if self.settings_store.trim_old_thinking(chat.user_id).await? {
            pick_thinking_boundary(
                &messages,
                chat.thinking_trimmed_up_to_message_id,
                fresh_thinking_budget_chars(context),
                FRESH_THINKING_CHARS, // what an untrimmed trace replays at while the setting is on
            )
        } else {
            None
        };
        if results.is_none() && thinking.is_none() {
            return Ok(None);
        }
        Ok(Some(ClearPlan {
            cleared_up_to: results.map(|(id, _)| id),
            thinking_trimmed_up_to: thinking.map(|(id, _)| id),
            freed_results: results.map_or(0, |(_, freed)| freed),
            freed_thinking: thinking.map_or(0, |(_, freed)| freed),
        }))
    }
}

/// A result shorter than this is left as it is: a stub would not be much smaller.
const MIN_CLEARED_CHARS: usize = 600;
/// What the fresh (uncleared) tool results may fill, as a fraction of the context window in tokens.
const FRESH_TOKEN_FRACTION: f64 = 0.35;
/// Measured on a real tool-heavy chat (prompt growth against the characters added, 26 turns): a
/// median of 4.0 characters a token, 4.1 for the large file reads. Shell and web output runs
/// lower, which makes this estimate cautious about how much a clearing pass freed.
const CHARS_PER_TOKEN: f64 = 4.0;
/// A pass that would free less than this isn't worth a prompt rewrite.
const MIN_SAVED_CHARS: usize = 6_000;
const STUB_ARGUMENTS_CHARS: usize = 160;
const STUB_BEGINNING_CHARS: usize = 100;

/// How many characters of tool results the newest part of the history may keep in full.
fn fresh_budget_chars(context_tokens: u64) -> usize {
    (context_tokens as f64 * FRESH_TOKEN_FRACTION * CHARS_PER_TOKEN) as usize
}

/// A rough token count for characters taken out of a prompt, to judge whether clearing was
/// enough before the next measured prompt size says so.
pub(super) fn estimate_tokens(chars: usize) -> u64 {
    (chars as f64 / CHARS_PER_TOKEN) as u64
}

/// What a trimmed thinking trace is replayed at: its last this many characters.
const TRIMMED_THINKING_CHARS: usize = 1_500;
/// What one fresh trace is replayed at while old ones are trimmed (the default is lower, so the
/// room trimming frees can go to the newest reasoning).
const FRESH_THINKING_CHARS: usize = 24_000;
/// What the fresh traces together may fill, as a fraction of the context window in tokens.
const FRESH_THINKING_FRACTION: f64 = 0.10;

/// How many characters of thinking the newest part of the history may replay in full.
fn fresh_thinking_budget_chars(context_tokens: u64) -> usize {
    (context_tokens as f64 * FRESH_THINKING_FRACTION * CHARS_PER_TOKEN) as usize
}

/// The cap one assistant message's thinking is replayed at when old thinking is trimmed.
pub(in crate::facade::agent) fn thinking_cap(message_id: i64, trimmed_up_to: Option<i64>) -> usize {
    if trimmed_up_to.is_some_and(|boundary| message_id <= boundary) {
        TRIMMED_THINKING_CHARS
    } else {
        FRESH_THINKING_CHARS
    }
}

/// The boundary for thinking, the way `pick_boundary` finds one for tool results: from the newest
/// assistant message back, traces are kept at `FRESH_THINKING_CHARS` until they fill
/// `budget_chars`, and the first that doesn't fit and everything older is trimmed. The newest
/// trace is always kept. `current_cap` is what an untrimmed trace is replayed at today, to say how
/// much the pass frees. Short traces don't count.
fn pick_thinking_boundary(messages: &[Message], already: Option<i64>, budget_chars: usize, current_cap: usize) -> Option<(i64, usize)> {
    let long = |m: &&Message| m.role == "assistant" && m.thinking.as_deref().is_some_and(|t| t.chars().count() > TRIMMED_THINKING_CHARS);
    let len = |m: &Message| m.thinking.as_deref().map_or(0, |t| t.trim().chars().count());
    let mut kept = 0usize;
    let mut boundary = None;
    for (seen, message) in messages.iter().rev().filter(long).enumerate() {
        if boundary.is_some() {
            break;
        }
        let replayed = len(message).min(FRESH_THINKING_CHARS);
        if seen > 0 && kept + replayed > budget_chars {
            boundary = Some(message.id);
        } else {
            kept += replayed;
        }
    }
    let boundary = boundary.filter(|&id| already.is_none_or(|old| id > old))?;
    let freed: usize = messages
        .iter()
        .filter(long)
        .filter(|m| m.id <= boundary && already.is_none_or(|old| m.id > old))
        .map(|m| len(m).min(current_cap).saturating_sub(TRIMMED_THINKING_CHARS))
        .sum();
    (freed >= MIN_SAVED_CHARS).then_some((boundary, freed))
}

fn clearable(message: &Message) -> bool {
    message.role == "tool" && message.content.chars().count() >= MIN_CLEARED_CHARS
}

/// The new boundary, and how many characters it frees, for `messages` (oldest first, everything
/// after the compaction boundary): walks from the newest tool result back, keeps results in full
/// until they fill `budget_chars`, and puts the boundary at the first one that doesn't fit, so it
/// and everything older is cleared. The newest result is always kept in full, however large, and
/// small ones don't count. `None` when nothing new would be cleared or it frees too little.
fn pick_boundary(messages: &[Message], already: Option<i64>, budget_chars: usize) -> Option<(i64, usize)> {
    let mut kept = 0usize;
    let mut boundary = None;
    let mut seen_newest = false;
    for message in messages.iter().rev().filter(|m| clearable(m)) {
        let len = message.content.chars().count();
        if !seen_newest {
            seen_newest = true;
            kept += len;
            continue;
        }
        if boundary.is_none() {
            if kept + len <= budget_chars {
                kept += len;
                continue;
            }
            boundary = Some(message.id);
        }
    }
    let boundary = boundary.filter(|&id| already.is_none_or(|old| id > old))?;
    let freed: usize = messages
        .iter()
        .filter(|m| clearable(m) && m.id <= boundary && already.is_none_or(|old| m.id > old))
        .map(|m| m.content.chars().count())
        .sum();
    (freed >= MIN_SAVED_CHARS).then_some((boundary, freed))
}

/// Replaces the content of every large tool result up to and including `cleared_up_to` with its
/// stub. `messages` is oldest first, so a tool result can be matched to the call that asked for it
/// (the n-th result after an assistant message answers its n-th call).
pub(in crate::facade::agent) fn stub_cleared(messages: &mut [Message], cleared_up_to: Option<i64>) {
    let Some(limit) = cleared_up_to else { return };
    let mut calls: VecDeque<ToolCallOut> = VecDeque::new();
    for message in messages.iter_mut() {
        if message.role == "assistant" && !message.tool_calls.is_empty() {
            calls = message.tool_calls.iter().map(|c| ToolCallOut { tool_name: c.tool_name.clone(), arguments: c.arguments.clone() }).collect();
        } else if message.role == "tool" {
            // A result is paired with the call that asked for it by position; a name that does not match means the pairing is
            // off (a gap in the batch), and a wrong path in a stub would send the model to the wrong file
            let call = calls.pop_front().filter(|c| message.tool_name.as_deref().is_none_or(|name| name == c.tool_name));
            if message.id <= limit && clearable(message) {
                message.content = stub(message, call.as_ref());
            }
        }
    }
}

fn stub(message: &Message, call: Option<&ToolCallOut>) -> String {
    let name = message.tool_name.as_deref().or(call.map(|c| c.tool_name.as_str())).unwrap_or("a tool");
    let arguments = call
        .map(|c| truncated(&c.arguments.to_string(), STUB_ARGUMENTS_CHARS))
        .unwrap_or_default();
    let beginning = truncated(&first_line(&message.content), STUB_BEGINNING_CHARS);
    format!(
        "[Result cleared to save context: {name}({arguments}) returned {} characters, beginning \"{beginning}\". \
         Call it again if you need the content.]",
        message.content.chars().count()
    )
}

/// The first non-empty line of what the tool returned. A result is stored as JSON whose
/// `content` field holds the text (a file's, a command's output), so that text is what counts.
fn first_line(stored: &str) -> String {
    let text = serde_json::from_str::<Value>(stored)
        .ok()
        .and_then(|value| value.get("content").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| stored.to_string());
    text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or_default().to_string()
}

fn truncated(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn message(id: i64, role: &str, content: String, calls: Vec<ToolCallOut>) -> Message {
        Message {
            id,
            chat_id: 1,
            role: role.to_string(),
            content,
            tool_name: (role == "tool").then(|| "storage.read_file".to_string()),
            created_at: Utc::now(),
            thinking: None,
            thought_duration_ms: None,
            tool_success: None,
            tool_denied: false,
            tool_calls: calls,
            images: vec![],
            file_ids: vec![],
            prompt_tokens: None,
            eval_tokens: None,
        }
    }

    fn call(path: &str) -> ToolCallOut {
        ToolCallOut { tool_name: "storage.read_file".to_string(), arguments: json!({"path": path}) }
    }

    fn read_result(len: usize) -> String {
        json!({"content": format!("fn first_line() {{}}\n{}", "x".repeat(len)), "truncated": false}).to_string()
    }

    /// assistant call + big result, `n` times, ids 1.. in pairs
    fn history(n: usize, len: usize) -> Vec<Message> {
        (0..n)
            .flat_map(|i| {
                vec![
                    message(2 * i as i64 + 1, "assistant", String::new(), vec![call(&format!("/f{i}.rs"))]),
                    message(2 * i as i64 + 2, "tool", read_result(len), vec![]),
                ]
            })
            .collect()
    }

    #[test]
    fn the_boundary_keeps_the_newest_results_within_the_budget_and_always_the_last() {
        let messages = history(5, 10_000); // results at ids 2,4,6,8,10, about 10k chars each
        // room for two in full: ids 10 and 8 stay, 6 and older are cleared
        let (boundary, freed) = pick_boundary(&messages, None, 21_000).unwrap();
        assert_eq!(boundary, 6);
        assert!(freed > 30_000);
        // a budget smaller than one result still keeps the newest in full
        assert_eq!(pick_boundary(&messages, None, 10).unwrap().0, 8);
        // everything fits: nothing to do
        assert_eq!(pick_boundary(&messages, None, 1_000_000), None);
        // the boundary only moves forward
        assert_eq!(pick_boundary(&messages, Some(6), 21_000), None);
        assert_eq!(pick_boundary(&messages, Some(4), 21_000).unwrap().0, 6);
    }

    #[test]
    fn small_results_are_neither_counted_nor_cleared_and_a_tiny_gain_is_skipped() {
        let mut messages = history(3, 10_000);
        messages.insert(1, message(100, "tool", "ok".to_string(), vec![]));
        let (boundary, _) = pick_boundary(&messages, None, 12_000).unwrap();
        assert_eq!(boundary, 4);
        stub_cleared(&mut messages, Some(boundary));
        assert_eq!(messages[1].content, "ok");

        let small = history(3, 1_000); // 3k chars of results: below the gain worth a rewrite
        assert_eq!(pick_boundary(&small, None, 100), None);
    }

    #[test]
    fn a_stub_names_the_call_and_the_start_and_does_not_change() {
        let mut messages = history(2, 5_000);
        stub_cleared(&mut messages, Some(2));
        let stub = &messages[1].content;
        assert!(stub.contains(r#"storage.read_file({"path":"/f0.rs"})"#), "{stub}");
        assert!(stub.contains("returned"), "{stub}");
        assert!(stub.contains("beginning \"fn first_line() {}\""), "{stub}");
        assert!(stub.len() < 400);
        // the result after the boundary is untouched
        assert!(messages[3].content.len() > 5_000);
        // building it again gives the same bytes
        let mut again = history(2, 5_000);
        stub_cleared(&mut again, Some(2));
        assert_eq!(again[1].content, *stub);
        // and stubbing what is already a stub changes nothing (short, so not clearable)
        let before = messages[1].content.clone();
        stub_cleared(&mut messages, Some(2));
        assert_eq!(messages[1].content, before);
    }

    fn thinker(id: i64, len: usize) -> Message {
        let mut m = message(id, "assistant", "text".to_string(), vec![]);
        m.thinking = Some("t".repeat(len));
        m
    }

    #[test]
    fn old_thinking_is_trimmed_past_a_budget_and_the_newest_trace_is_kept() {
        let messages: Vec<Message> = (1..=6).map(|i| thinker(i, 9_000)).collect();
        // room for two traces in full: 6 and 5 stay, 4 and older are trimmed
        let (boundary, freed) = pick_thinking_boundary(&messages, None, 18_000, 10_000).unwrap();
        assert_eq!(boundary, 4);
        assert_eq!(freed, 4 * (9_000 - TRIMMED_THINKING_CHARS));
        assert_eq!(pick_thinking_boundary(&messages, None, 10, 10_000).unwrap().0, 5);
        assert_eq!(pick_thinking_boundary(&messages, None, 1_000_000, 10_000), None);
        assert_eq!(pick_thinking_boundary(&messages, Some(4), 18_000, 10_000), None);
        // short traces are neither counted nor trimmed
        let short: Vec<Message> = (1..=6).map(|i| thinker(i, 1_000)).collect();
        assert_eq!(pick_thinking_boundary(&short, None, 10, 10_000), None);
        assert_eq!(thinking_cap(4, Some(4)), TRIMMED_THINKING_CHARS);
        assert_eq!(thinking_cap(5, Some(4)), FRESH_THINKING_CHARS);
        assert_eq!(thinking_cap(1, None), FRESH_THINKING_CHARS);
    }

    #[test]
    fn a_result_whose_tool_differs_from_the_paired_call_gets_no_arguments() {
        let mut other = call("/a.rs");
        other.tool_name = "storage.find_files".to_string();
        let mut messages = vec![message(1, "assistant", String::new(), vec![other]), message(2, "tool", read_result(900), vec![])];
        stub_cleared(&mut messages, Some(2));
        assert!(messages[1].content.contains("storage.read_file()"), "{}", messages[1].content);
        assert!(!messages[1].content.contains("/a.rs"));
    }

    #[test]
    fn results_are_matched_to_their_own_call_in_a_batch() {
        let mut messages = vec![
            message(1, "assistant", String::new(), vec![call("/a.rs"), call("/b.rs")]),
            message(2, "tool", read_result(900), vec![]),
            message(3, "tool", read_result(900), vec![]),
        ];
        stub_cleared(&mut messages, Some(3));
        assert!(messages[1].content.contains("/a.rs"));
        assert!(messages[2].content.contains("/b.rs"));
    }
}
