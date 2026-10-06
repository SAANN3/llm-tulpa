//! Keeping a chat inside its context window: when the prompt passes the trigger, old tool results
//! are cleared (`clearing`), the model is asked for its notes (`notes`), and when that was not enough
//! the oldest messages are folded into a summary and key facts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Deserialize;

use super::history::History;
use super::model_call::ModelCall;
use super::prompts;
use crate::facade::one_shot::OneShot;
use crate::services::chat_store::{ChatFacts, ChatStore, Message};
use crate::services::error::ErrorService;
use crate::services::llm::{ChatMessage, ThinkChoice};
use crate::services::settings_store::SettingsStore;
use crate::services::tools::ToolService;
use clearing::CLEARED_ENOUGH_FRACTION;

pub(super) mod clearing;
mod notes;

/// `trigger_tokens`/`keep_chars` (below) are derived from the
/// real, configured context window rather than hardcoded — otherwise they'd silently
/// drift out of sync with `OLLAMA_CONTEXT_LENGTH` if that's ever changed without also
/// hand-editing these. `TRIGGER_FRACTION` leaves real headroom under the ceiling
/// (rather than waiting until a turn is already at risk of the same truncation failure
/// `storage::read_file`'s size cap exists to avoid downstream of); `KEEP_CHARS_PER_TOKEN`
/// is a rough token-to-char proxy (same reasoning as `storage::read_file`'s
/// `MAX_READ_CHARS`), not an exact budget.
///
/// `KEEP_CHARS_PER_TOKEN` needs real margin below the *actual* chars-per-token ratio
/// of whatever content a chat holds, not just a plausible-looking average — an
/// agentic, tool/code-heavy chat's real content tokenizes far less efficiently than
/// prose (observed ~2.3 chars/token on a real chat's `os.execute_command`/
/// `web.request`-heavy tail, against a naive ~4+ for plain English). Too little
/// margin here means `keep_chars` ends up corresponding to nearly the
/// *entire* context window in real tokens instead of a meaningfully smaller kept
/// slice — the kept tail then sits right at that ceiling with almost nothing left
/// eligible to fold, so compaction re-triggers on nearly every turn (each one
/// changing the summary/facts and re-paying a full prompt-cache miss) instead of
/// settling comfortably below the trigger for a while. Undershooting the other way
/// (folding somewhat more than strictly necessary) has no correctness risk — it only
/// costs a bit of verbatim detail that the summary/facts channel already exists to
/// preserve.
///
/// 1.2 was still too close: a real agentic chat measured ~1.9 chars/token, so the
/// kept tail came to ~82k tokens, which with ~10k tokens of tools and system prompt
/// sat at the 0.70 trigger itself. Each fold then removed only 1-4 messages, the next
/// turn crossed the trigger again, and every fold (it rewrites the summary at the very
/// front of the prompt) cost a full ~150s re-evaluation at ~92k tokens, four times in
/// nine minutes. At 0.6 a fold leaves roughly 40-50k tokens of tail, so the next one is
/// tens of thousands of tokens of growth away.
const TRIGGER_FRACTION: f64 = 0.70;
const KEEP_CHARS_PER_TOKEN: f64 = 0.6;
/// After a failed fold, the prompt has to grow by this fraction of the window before the next try.
const COMPACTION_RETRY_GROWTH: f64 = 0.05;

#[derive(Clone)]
pub(super) struct Compaction {
    chat_store: Arc<ChatStore>,
    settings_store: Arc<SettingsStore>,
    tools: Arc<ToolService>,
    model: ModelCall,
    history: History,
    /// One-shot calls (the compaction summary and the key facts), on the same providers.
    one_shot: OneShot,
    /// Chats whose last fold failed, with the prompt size it failed at: `maybe_compact` leaves them alone
    /// until the prompt has grown by `COMPACTION_RETRY_GROWTH` of the window. Without it a summarizer that
    /// keeps being refused would cost a notes request and two summary calls on every single turn.
    backoff: Arc<Mutex<HashMap<i64, u64>>>,
}

impl Compaction {
    pub(super) fn new(
        chat_store: Arc<ChatStore>,
        settings_store: Arc<SettingsStore>,
        tools: Arc<ToolService>,
        model: ModelCall,
        history: History,
        one_shot: OneShot,
    ) -> Self {
        Self { chat_store, settings_store, tools, model, history, one_shot, backoff: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Checks whether the turn that just finished (or last turn before starting a new one) pushed prompt usage over
    /// `trigger_tokens` and, if so, compacts older history into
    /// `Chat::summary` before returning — so the *next* request (a fresh turn, or
    /// another `continue_chat` later in the same tool-calling round) builds a smaller
    /// prompt via `History::for_chat`. Best-effort: a failure here doesn't fail the turn
    /// that already succeeded, it just means history stays as big as it is and gets
    /// another chance to trigger this again.
    pub(super) async fn maybe_compact(&self, chat_id: i64, prompt_eval_count: Option<u64>, think: Option<ThinkChoice>) {
        // The thresholds follow the window the chat's own model runs under (its launch profile's
        // context), not one global number. A failed lookup is a reason to skip this check, not to fail.
        let context = match self.chat_store.chat(chat_id).await {
            Ok(chat) => match self.model.context_of(&chat).await {
                Ok(context) => context,
                Err(_) => return,
            },
            Err(_) => return,
        };
        let trigger_tokens = (context as f64 * TRIGGER_FRACTION) as u64;
        if prompt_eval_count.unwrap_or(0) < trigger_tokens {
            return;
        }
        // A fold that just failed is not retried until the prompt has grown some, see `compaction_backoff`
        let failed_at = self.backoff.lock().unwrap().get(&chat_id).copied();
        if failed_at.is_some_and(|at| prompt_eval_count.unwrap_or(0) < at + (context as f64 * COMPACTION_RETRY_GROWTH) as u64) {
            return;
        }

        tracing::info!(
            chat_id,
            prompt_eval_count,
            trigger_threshold = trigger_tokens,
            "compaction triggered for chat_id {chat_id}"
        );

        // Dropping old tool results is cheaper than a model-written summary and loses nothing the
        // model can't fetch again; fold only when that wasn't enough
        let mut notes_saved = false;
        match self.plan_clearing(chat_id, context).await {
            Ok(Some(plan)) => {
                // The notes are asked for while the prompt is still the one the model server holds:
                // applying the plan rewrites it, and the request would be a cold read of the whole history
                self.save_notes_before_fold(chat_id, think.clone(), prompt_eval_count).await;
                notes_saved = true;
                let freed_chars = plan.freed_chars();
                match self.chat_store.set_context_boundaries(chat_id, plan.cleared_up_to, plan.thinking_trimmed_up_to).await {
                    Ok(()) => {
                        let estimated = prompt_eval_count.unwrap_or(0).saturating_sub(clearing::estimate_tokens(freed_chars));
                        let target = (context as f64 * CLEARED_ENOUGH_FRACTION) as u64;
                        if estimated < target {
                            tracing::info!(chat_id, freed_chars, estimated, "old tool results cleared, no fold needed");
                            return;
                        }
                        tracing::info!(chat_id, freed_chars, estimated, "old tool results cleared, still above the target: folding");
                    }
                    Err(e) => tracing::warn!("clearing old tool results failed for chat {chat_id}: {e:?}"),
                }
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(
                "clearing old tool results failed for chat {chat_id}: {}",
                e.message.as_deref().unwrap_or("unknown error")
            ),
        }

        match self.compact(chat_id, (context as f64 * KEEP_CHARS_PER_TOKEN) as usize, notes_saved, think, prompt_eval_count).await {
            Ok(()) => {
                self.backoff.lock().unwrap().remove(&chat_id);
            }
            Err(e) => {
                tracing::warn!(
                    "history compaction failed for chat {chat_id}: {}",
                    e.message.as_deref().unwrap_or("unknown error")
                );
                self.backoff.lock().unwrap().insert(chat_id, prompt_eval_count.unwrap_or(0));
            }
        }
    }

    /// Folds the oldest not-yet-summarized messages into `Chat::summary` until what's
    /// left is under `keep_chars`, merging in the existing summary (if any)
    /// rather than discarding it. No-ops if everything already fits — that means
    /// `trigger_tokens` fired on a single outsized turn rather than a long
    /// history, which folding can't help with.
    async fn compact(&self, chat_id: i64, keep_chars: usize, notes_saved: bool, think: Option<ThinkChoice>, known_prompt_tokens: Option<u64>) -> Result<(), ErrorService> {
        let chat = self.chat_store.chat(chat_id).await?;
        let after_id = chat.summary_up_to_message_id.unwrap_or(0);

        // Oldest first: easier to reason about a boundary over
        let mut messages = self.chat_store.messages_after(chat_id, after_id).await?;
        // What stays in the prompt is judged at its real size, stubs included
        clearing::stub_cleared(&mut messages, chat.cleared_up_to_message_id);

        let trim_thinking = self.settings_store.trim_old_thinking(chat.user_id).await?;
        let sizes: Vec<usize> = messages
            .iter()
            .map(|message| {
                let thinking = message.thinking.as_deref().map_or(0, str::len);
                message.content.len()
                    + if trim_thinking {
                        thinking.min(clearing::thinking_cap(message.id, chat.thinking_trimmed_up_to_message_id))
                    } else {
                        thinking
                    }
                    + message.images.iter().map(String::len).sum::<usize>()
            })
            .collect();
        let split_at = pick_compaction_boundary(&sizes, keep_chars);

        if split_at == 0 {
            tracing::info!(
                chat_id,
                total_messages = messages.len(),
                keep_chars,
                "compaction skipped: recent history already fits within keep_chars"
            );
            return Ok(());
        }

        let to_fold = &messages[..split_at];
        let new_boundary_id = to_fold.last().map(|m| m.id).unwrap_or(after_id);

        tracing::info!(
            chat_id,
            messages_folded = to_fold.len(),
            messages_retained = messages.len() - to_fold.len(),
            had_prior_summary = chat.summary.is_some(),
            "calling summarize for chat_id {chat_id}"
        );

        // Best-effort, and before the fold: when nothing was cleared first, the prompt the model
        // server holds is still the one about to be folded, so asking for the notes costs almost
        // nothing. (After a clearing pass they were already asked for, ahead of it.)
        if !notes_saved {
            self.save_notes_before_fold(chat_id, think.clone(), known_prompt_tokens).await;
        }

        let summary = self.summarize(chat.summary.clone(), to_fold, &chat.provider, &chat.model).await?;
        let facts = self.extract_facts(to_fold, chat.key_facts.clone(), &chat.provider, &chat.model).await;
        let existing_facts_count = chat.key_facts.as_ref().map_or(0, |f| f.facts.len());
        let merged = Self::merge_facts(chat.key_facts.clone().unwrap_or_default(), facts.goal, facts.facts);
        // `merge_facts` only ever appends, so this can't underflow in practice — saturating
        // anyway rather than trusting that invariant never breaks in the future.
        let facts_added = merged.facts.len().saturating_sub(existing_facts_count);

        self.chat_store
            .set_summary(chat_id, summary, merged, new_boundary_id)
            .await?;

        tracing::info!(chat_id, new_boundary_id, facts_added, "compaction finished for chat_id {chat_id}");

        Ok(())
    }

    /// Produces an updated summary covering `existing_summary` (if any) plus every
    /// message in `to_fold`, via a plain (no tools) Ollama call — not part of the
    /// visible conversation, so it doesn't go through `advance`/get persisted as a chat
    /// message itself.
    async fn summarize(
        &self,
        existing_summary: Option<String>,
        to_fold: &[Message],
        provider: &str,
        model: &str,
    ) -> Result<String, ErrorService> {
        // The image data itself never goes into the transcript (it's not text, and this
        // call carries no vision guarantee) — but a message that had one needs to say
        // so, or folding it away loses any trace it ever happened, silently.
        let transcript = Self::transcript_of(to_fold);

        let prior = existing_summary.map(|summary| prompts::prior_summary_block(&summary)).unwrap_or_default();

        // `think: false` (what a one-shot call always does) — measured head-to-head against the same
        // real fold-candidate messages (`summarize_bench`, since deleted): thinking cost ~2x the time
        // and token budget, and produced a *shorter, less detailed* final summary — the deliberation
        // ate the token budget that would've otherwise gone into exact struct/field names and
        // per-tool specifics, which is exactly what the system prompt asks it to preserve. Reasoning
        // first turned out to hurt the thing it was meant to help here, not just cost more.
        //
        // One retry with a sharper closing: a bad reply (a tool call, a continuation, missing
        // sections) is not stored, because everything folded is lost with it. When both are refused
        // the caller keeps the previous summary and boundary, so history stays as it is and the next
        // trigger tries again.
        self.one_shot
            .ask_checked(
                provider,
                model,
                prompts::SUMMARIZER_SYSTEM.to_string(),
                |closing| ChatMessage::user(prompts::summarizer_user(&prior, &transcript, closing)),
                &[prompts::SUMMARY_CLOSING, prompts::SUMMARY_SHARPER_CLOSING],
                summary_problem,
            )
            .await
    }

    /// Returns a plain-text transcript of messages suitable for both summarize and
    /// extract-facts prompts — same role annotations and image/mark conventions as
    /// summarize's own transcript, factored out so the code isn't duplicated.
    fn transcript_of(to_fold: &[Message]) -> String {
        to_fold
            .iter()
            .map(|message| match message.role.as_str() {
                "tool" => format!(
                    "[tool result — {}]: {}",
                    message.tool_name.as_deref().unwrap_or("?"),
                    message.content
                ),
                role if !message.images.is_empty() => format!(
                    "[{role}, {} image(s) attached]: {}",
                    message.images.len(),
                    message.content
                ),
                role => format!("[{role}]: {}", message.content),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Extracts new key facts from the fold, producing a `ChatFacts` with optional
    /// `goal` (only if existing goal is absent) and a list of new facts.
    ///
    /// Best-effort: on parse failure or Ollama error, logs a warning and falls back
    /// to the existing facts (cloned, or `ChatFacts::default()` if none). This means
    /// the fold still proceeds even when fact extraction fails — the model just
    /// doesn't get better-structured context on the next fold until extraction works.
    async fn extract_facts(
        &self,
        to_fold: &[Message],
        existing_key_facts: Option<ChatFacts>,
        provider: &str,
        model: &str,
    ) -> ChatFacts {
        let existing = existing_key_facts.as_ref();
        let existing_goal = existing.and_then(|f| f.goal.clone());
        let existing_facts_str = prompts::existing_facts_block(existing.map_or(&[][..], |f| f.facts.as_slice()));

        let transcript = Self::transcript_of(to_fold);

        let system = prompts::facts_system(&existing_facts_str);
        let user = ChatMessage::user(prompts::facts_user(existing_goal.as_deref(), &transcript));

        // Best-effort (see above): a provider that can't be found is the same failure as one that
        // can't answer, and falls back the same way.
        let result = self.one_shot.ask(provider, model, system, user).await;
        match result {
            Err(err) => {
                let es: ErrorService = err;
                tracing::warn!(
                    error = es.message.as_deref().unwrap_or("unknown error"),
                    "fact extraction ollama call failed, using existing facts"
                );
                ChatFacts {
                    goal: existing_goal,
                    facts: existing.map_or(vec![], |f| f.facts.clone()),
                }
            }
            Ok(response) => {
                let raw = response.message.content;

                match Self::parse_extracted_facts(&raw) {
                    Ok((trimmed_goal, new_facts)) => {
                        if trimmed_goal.is_none() && existing_goal.is_some() {
                            tracing::debug!("extracted goal was empty/none, keeping existing");
                        }

                        if new_facts.is_empty() && trimmed_goal.is_none() {
                            tracing::debug!("extracted facts empty");
                        } else {
                            tracing::info!(
                                facts_extracted = new_facts.len(),
                                "successfully extracted facts from fold"
                            );
                        }

                        ChatFacts {
                            goal: trimmed_goal,
                            facts: new_facts,
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            raw = %raw,
                            "fact extraction parse failed, using existing facts"
                        );
                        ChatFacts {
                            goal: existing_goal,
                            facts: existing.map_or(vec![], |f| f.facts.clone()),
                        }
                    }
                }
            }
        }
    }

    /// Strips a leading/trailing ```` ```json ```` or ```` ``` ```` fence if the model wrapped
    /// its output in one, otherwise returns the trimmed input unchanged.
    fn strip_json_fences(raw: &str) -> &str {
        raw.strip_prefix("```json")
            .or_else(|| raw.strip_prefix("```"))
            .map(|s| s.trim_start().strip_suffix("```").map(|s| s.trim()).unwrap_or(s.trim()))
            .unwrap_or(raw.trim())
    }

    /// Parses `extract_facts`'s raw Ollama response into `(goal, facts)`, tolerating an
    /// optional code fence around the JSON. Trims and drops empty entries so callers never
    /// see whitespace-only facts or an empty-string goal. A pure function (no I/O, no
    /// `self`) so it's unit-testable without a live Ollama call — see the tests below.
    fn parse_extracted_facts(raw: &str) -> Result<(Option<String>, Vec<String>), serde_json::Error> {
        #[derive(Deserialize)]
        struct WireFacts {
            goal: Option<String>,
            facts: Vec<String>,
        }

        let cleaned = Self::strip_json_fences(raw);
        let wire: WireFacts = serde_json::from_str(cleaned)?;

        let goal = wire
            .goal
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_string);

        let facts = wire
            .facts
            .into_iter()
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty())
            .collect();

        Ok((goal, facts))
    }

    /// Deterministic merge of existing facts with a fresh extraction.
    ///
    /// - goal: keep existing.goal if set and non-empty, else take new_goal (if non-empty).
    /// - facts: push each new fact, skipping empty/whitespace-only ones and any whose
    ///   trimmed form case-insensitively equals an existing entry's trimmed form.
    /// - Never reorder, never rewrite, never drop existing entries.
    ///
    /// This is a pure function so it's unit-testable and independent of any service
    /// wiring — correctness-critical invariants are enforced here, not by the model.
    fn merge_facts(existing: ChatFacts, new_goal: Option<String>, new_facts: Vec<String>) -> ChatFacts {
        let goal = existing.goal.or_else(|| {
            let trimmed = new_goal?.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });

        let mut facts = existing.facts;
        for new_fact in new_facts {
            let trimmed = new_fact.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            let exists = facts.iter().any(|existing_fact| {
                existing_fact.trim().to_lowercase() == trimmed.to_lowercase()
            });
            if !exists {
                facts.push(trimmed);
            }
        }

        ChatFacts { goal, facts }
    }
}

/// Why a summarizer reply can't be stored as the compaction summary, or `None` when it can.
/// A stored bad summary replaces everything folded into it, so this is strict: chat 263's was
/// a text tool call. A real tool call, the model's own tool-call tags in the text, or a reply
/// missing the three sections all count.
fn summary_problem(message: &ChatMessage, markers: &[String]) -> Option<&'static str> {
    let content = message.content.trim();
    if content.is_empty() {
        return Some("empty reply");
    }
    if message.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty())
        || markers.iter().any(|marker| content.contains(marker.as_str()))
    {
        return Some("tool call instead of a summary");
    }
    let upper = content.to_uppercase();
    let has_sections = ["ESTABLISHED FACTS", "COMPLETED CHANGES", "CURRENT UNSOLVED OBJECTIVE"]
        .iter()
        .all(|header| upper.contains(header));
    (!has_sections).then_some("the three sections are missing")
}

/// Pure boundary-selection for `Agent::compact` — pulled out of it so the arithmetic is
/// checkable on its own, without a live `ChatStore`/`OllamaService`. `sizes` is each
/// message's weight (content + thinking chars, plus each attached image's base64
/// length — a proportional stand-in for its real token cost, not an exact one, same
/// spirit as `KEEP_CHARS_PER_TOKEN` below), oldest first (same order `compact` reverses
/// its messages into). Walks from the newest (the end) backward, keeping a message only if it still
/// fits under `keep_chars` alongside everything newer already kept; returns the index
/// where `[0, index)` should be folded away and `[index, len)` kept verbatim. `0` means
/// nothing needs folding — everything already fits.
fn pick_compaction_boundary(sizes: &[usize], keep_chars: usize) -> usize {
    let mut kept_chars = 0usize;
    let mut split_at = sizes.len();

    for (index, &size) in sizes.iter().enumerate().rev() {
        if kept_chars + size > keep_chars {
            break;
        }
        kept_chars += size;
        split_at = index;
    }

    split_at
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_pick_compaction_boundary_logic() {
        let sizes = vec![1000, 2000, 3000, 4000];
        let split = pick_compaction_boundary(&sizes, 5000);
        assert_eq!(split, 3);

        let split_all = pick_compaction_boundary(&sizes, 15000);
        assert_eq!(split_all, 0);
    }

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

    const GOOD_SUMMARY: &str = "1. ESTABLISHED FACTS & FINDINGS: x\n2. COMPLETED CHANGES: none\n3. CURRENT UNSOLVED OBJECTIVE: y";

    #[test]
    fn summary_problem_accepts_a_structured_summary() {
        let markers = vec!["<tool_call>".to_string(), "</tool_call>".to_string()];
        assert_eq!(summary_problem(&reply(GOOD_SUMMARY), &markers), None);
    }

    #[test]
    fn summary_problem_rejects_chat_263s_stored_summary() {
        let markers = vec!["<tool_call>".to_string()];
        let bad = "I'll continue reading the agent.rs file.\n<tool_call>\n<function=read_file>\n</function>";
        assert!(summary_problem(&reply(bad), &markers).is_some());
    }

    #[test]
    fn summary_problem_rejects_empty_unstructured_and_real_tool_calls() {
        assert!(summary_problem(&reply("  \n"), &[]).is_some());
        assert!(summary_problem(&reply("The user wanted a server-side turn."), &[]).is_some());
        let mut with_call = reply(GOOD_SUMMARY);
        with_call.tool_calls = Some(vec![crate::services::llm::ModelToolCall {
            id: "1".into(),
            function: crate::services::llm::ModelToolCallFunction {
                index: None,
                name: "storage.read_file".into(),
                arguments: json!({}),
            },
        }]);
        assert!(summary_problem(&with_call, &[]).is_some());
    }
}

