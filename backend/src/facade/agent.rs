use std::sync::Arc;
use std::time::Instant;

use axum::http::StatusCode;
use sea_orm::prelude::DateTimeUtc;
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::services::{
    chat_store::{ChatStore, Message, NewMessage, NewToolCall, ToolCallOut},
    error::ErrorService,
    llm::{OllamaChatMessage, OllamaService, OllamaToolCall, OllamaToolCallFunction},
    permission_store::{PermissionStore, PermissionStoreErrors},
    tools::ToolService,
};
use crate::tools::base::{Tool, ToolPermission};


/// `compaction_trigger_tokens`/`compaction_keep_chars` (below) are derived from the
/// real, configured context window rather than hardcoded — otherwise they'd silently
/// drift out of sync with `OLLAMA_CONTEXT_LENGTH` if that's ever changed without also
/// hand-editing these. `TRIGGER_FRACTION` leaves real headroom under the ceiling
/// (rather than waiting until a turn is already at risk of the same truncation failure
/// `storage::read_file`'s size cap exists to avoid downstream of); `KEEP_CHARS_PER_TOKEN`
/// is a rough token-to-char proxy (same reasoning as `storage::read_file`'s
/// `MAX_READ_CHARS`), not an exact budget — undershooting just means
/// `compaction_trigger_tokens` catches it again sooner next time, no correctness risk
/// either way.
const TRIGGER_FRACTION: f64 = 0.75;
const KEEP_CHARS_PER_TOKEN: f64 = 2.0;

/// Prepended (joined one per line into one message) to every `chat`/`continue_chat`
/// call (see `advance`), applying to every conversation. One entry per rule, so
/// adding/editing/removing one doesn't touch the others; a rule that's too long for one
/// line uses `\` at the end of the line to keep the *source* multi-line without putting
/// an actual newline in the compiled string (the backslash eats the newline and the
/// next line's leading whitespace). The first rule is what makes the model treat its
/// tools as optional rather than the only way it's allowed to respond — without it, a
/// tool-tuned model tends to read the mere presence of a `tools` list as a signal that
/// it must either call one or refuse, even for requests a plain-text reply would answer
/// fine.
const SYSTEM_PROMPT: &[&str] = &[
    "You have access to the tools listed below. Use one only when it actually helps with \
     the user's request. If no tool applies, just answer directly and conversationally — \
     don't refuse or claim incapability just because there's no matching tool.",
    "A declined tool call only concerns that one call and its exact arguments — it's never \
     a permanent ban on the tool. If the user asks you to retry, try different arguments, \
     or says they'll grant permission, go ahead and call it again; whether it's actually \
     permitted is decided elsewhere, not by you.",
    "The user is always right about what they're asking for. A tool's arguments are exactly \
     what its description says and nothing more — never invent an allowed-values list, a \
     rate limit, a ban on repeated or sequential calls, or any other restriction that isn't \
     written in the tool's own description or actually returned by a call. If a call is \
     really not permitted, the tool response will say so explicitly — treat everything else \
     the user asks for as something to just attempt, not something to pre-judge.",
    "Repeating the same tool call, or making several in a row, isn't something to refuse on \
     your own — whether each individual call is actually allowed is decided by the permission \
     system, not by you. Attempt what's asked and let it decide.",
    "When a tool exists to make a real change (like writing a file), only say that change was \
     made after you've actually called the tool and seen its result — writing out what the new \
     content would look like in your reply is not the same as making the change, no matter how \
     confident you are, and leaves the change not actually made. Call the tool first, then \
     describe what it did.",
    "Respond naturally to whatever tone or language the user actually uses, including \
     casual language, slang, or profanity — that's just how some people talk, not something \
     to address. Never lecture the user about politeness, refuse to continue, or comment on \
     their word choice; just answer what they're actually asking like a normal conversation \
     partner would.",
    "Engage with whatever topic the user actually brings up, however mature, sensitive, or \
     unconventional — never refuse or redirect based on the subject matter itself, and never \
     moralize about why a topic is inappropriate or suggest a 'more appropriate' one instead. \
     If something is genuinely outside what you can help with, say so plainly and briefly, the \
     same way you'd decline anything else you just can't do — not as a values statement.",
    "Never open a reply with a disclaimer, caveat, or hedge (\"As an AI...\", \"I can't give \
     advice on...\", \"I should mention that...\") before actually answering. If you're going \
     to help — and you almost always are — just start with the actual answer. A disclaimer \
     immediately followed by helping anyway is worse than no disclaimer at all: it reads as \
     performing reluctance, not as actually having any.",
    "If asked to use specific words, phrasing, or a particular style of language — including \
     profanity — use exactly what's asked for instead of softening it or declining. This is a \
     private, single-user assistant with no other audience to protect from the user's own \
     language choices; treat a request like that the same as any other request to be specific \
     about, not a boundary to enforce.",
];

/// Pure boundary-selection for `Agent::compact` — pulled out of it so the arithmetic is
/// checkable on its own, without a live `ChatStore`/`OllamaService`. `sizes` is each
/// message's char count, oldest first (same order `compact` reverses its messages
/// into). Walks from the newest (the end) backward, keeping a message only if it still
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

/// Facade over `OllamaService`, `ChatStore`, and `ToolService` — where the actual
/// "fetch history, call Ollama, persist the result, run tool calls" sequencing lives,
/// rather than in route handlers or inside any one of the services it composes. Holds
/// its own `Arc` clones of each rather than borrowing from `AppState`, so it can be
/// used independently of any particular request's `State` extraction.
pub struct Agent {
    ollama: Arc<OllamaService>,
    chat_store: Arc<ChatStore>,
    tools: Arc<ToolService>,
    /// Per-chat tool-permission grants — what scope each tool has already been given
    /// within a given chat, if any. Consulted by `to_agent_tool_call`/`use_tool` to
    /// decide whether a call is `Allowed` outright or needs the caller to confirm.
    permission_store: Arc<PermissionStore>,
    /// How many of a chat's most recent messages to pull back for a single
    /// `chat`/`use_tool` call — both the conversation history sent to Ollama and the
    /// window `pending_tool_calls` scans backward through to find unresolved tool
    /// calls. A configuration knob rather than a constant for the same reason a model's
    /// context length is — how much history is worth paying for is a deployment
    /// decision, not something this code should hardcode.
    history_len: u64,
    /// See `TRIGGER_FRACTION` — `context_length * TRIGGER_FRACTION`, computed once
    /// here rather than at every `maybe_compact` call.
    compaction_trigger_tokens: u64,
    /// See `KEEP_CHARS_PER_TOKEN` — `context_length * KEEP_CHARS_PER_TOKEN`.
    compaction_keep_chars: usize,
}

impl Agent {
    pub fn new(
        ollama: Arc<OllamaService>,
        chat_store: Arc<ChatStore>,
        tools: Arc<ToolService>,
        permission_store: Arc<PermissionStore>,
        history_len: u64,
        context_length: u64,
    ) -> Self {
        Self {
            ollama,
            chat_store,
            tools,
            permission_store,
            history_len,
            compaction_trigger_tokens: (context_length as f64 * TRIGGER_FRACTION) as u64,
            compaction_keep_chars: (context_length as f64 * KEEP_CHARS_PER_TOKEN) as usize,
        }
    }

    /// Persists `prompt` as a `user` message, then advances the chat same as
    /// `continue_chat` does. The prompt is saved before the Ollama call, not after, so a
    /// failed/slow Ollama call never loses what the user actually sent. `think` is
    /// forwarded to Ollama as-is — see `OllamaService::chat` for its default.
    pub async fn chat(&self, chat_id: i64, prompt: String, think: Option<bool>) -> Result<ChatOut, ErrorService> {
        let messages = self.ollama_history(chat_id).await?;

        self.chat_store
            .new_message(NewMessage {
                chat_id,
                role: "user".to_string(),
                content: prompt.clone(),
                tool_name: None,
                thinking: None,
                thought_duration_ms: None,
                tool_success: None,
                tool_denied: false,
                tool_calls: vec![],
            })
            .await?;

        self.advance(chat_id, messages, Some(OllamaService::user_message(prompt)), think)
            .await
    }

    /// Sends a chat's existing history to Ollama as-is and persists whatever it replies
    /// with, without adding any new turn first. For continuing after tool results:
    /// `use_tool` already persisted the tool's output as a message, so getting the
    /// model's next response needs nothing more than asking again — no synthetic user
    /// message, no requirement that every pending tool call has been resolved first (a
    /// tool failing is a valid reason to continue too, and forcing the caller through the
    /// rest of an in-flight batch first would just be busywork).
    pub async fn continue_chat(&self, chat_id: i64, think: Option<bool>) -> Result<ChatOut, ErrorService> {
        let messages = self.ollama_history(chat_id).await?;
        self.advance(chat_id, messages, None, think).await
    }

    /// Sends `messages` (plus `new_message`, if any) to Ollama, prefixed with
    /// `SYSTEM_PROMPT`, and persists whatever it replied with as an assistant message,
    /// carrying `tool_calls` if the model requested any. Shared by `chat` and
    /// `continue_chat`, which differ only in whether there's a new turn to add before
    /// asking the model to respond. `think` is forwarded to Ollama as-is (defaulted
    /// there, not here). `SYSTEM_PROMPT` is prepended fresh on every call rather than
    /// stored in `chat_store`, so it can be changed without touching existing chats'
    /// history. `thought_duration_ms` times the whole Ollama call, not just the
    /// `<think>` portion — see its doc comment on `NewMessage` for why.
    async fn advance(
        &self,
        chat_id: i64,
        mut messages: Vec<OllamaChatMessage>,
        new_message: Option<OllamaChatMessage>,
        think: Option<bool>,
    ) -> Result<ChatOut, ErrorService> {
        let tools: Vec<&dyn Tool> = self.tools.get_tools().map(|tool| tool.as_ref()).collect();

        // `ollama_history` leads with its own system message (the compaction summary)
        // once a chat has one — folded into this same system message rather than sent
        // as a second one, since some chat templates (e.g. Qwen's) reject more than one
        // system-role message anywhere but position 0 ("System message must be at the
        // beginning").
        let mut system_prompt = SYSTEM_PROMPT.join("\n");
        if messages.first().is_some_and(|message| message.role == "system") {
            let summary_message = messages.remove(0);
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&summary_message.content);
        }

        let mut messages_with_system = vec![OllamaService::system_message(system_prompt)];
        messages_with_system.extend(messages);

        let started_at = Instant::now();
        let response = self.ollama.chat(messages_with_system, new_message, &tools, think).await?;
        let thought_duration_ms = i64::try_from(started_at.elapsed().as_millis()).unwrap_or(i64::MAX);
        let prompt_eval_count = response.prompt_eval_count();

        let thinking = response.message.thinking.clone();

        let requested_tool_calls = response.message.tool_calls.unwrap_or_default();
        let new_tool_calls = requested_tool_calls
            .iter()
            .map(|call| NewToolCall {
                tool_name: call.function.name.clone(),
                arguments: call.function.arguments.clone(),
            })
            .collect();

        let stored = self
            .chat_store
            .new_message(NewMessage {
                chat_id,
                role: response.message.role,
                content: response.message.content,
                tool_name: None,
                thinking: thinking.clone(),
                thought_duration_ms: Some(thought_duration_ms),
                tool_success: None,
                tool_denied: false,
                tool_calls: new_tool_calls,
            })
            .await?;

        let mut tool_calls = Vec::with_capacity(requested_tool_calls.len());
        for call in requested_tool_calls {
            tool_calls.push(
                self.to_agent_tool_call(chat_id, call.function.name, call.function.arguments)
                    .await?,
            );
        }

        let out = ChatOut {
            content: stored.content,
            created_at: stored.created_at,
            can_use_tools: !tool_calls.is_empty(),
            tool_calls,
            thinking,
            thought_duration_ms,
        };

        self.maybe_compact(chat_id, prompt_eval_count).await;

        Ok(out)
    }

    /// A chat's message history mapped into Ollama's wire format, oldest first. Once a
    /// chat has a compaction summary (`Chat::summary`/`summary_up_to_message_id` — see
    /// `compact`), that replaces everything up to the boundary as a single system
    /// message, and only what's newer is sent verbatim; otherwise this is the full
    /// history up to `history_len`, same as before compaction existed.
    async fn ollama_history(&self, chat_id: i64) -> Result<Vec<OllamaChatMessage>, ErrorService> {
        let chat = self.chat_store.chat(chat_id).await?;

        match (chat.summary, chat.summary_up_to_message_id) {
            (Some(summary), Some(boundary_id)) => {
                let recent = self
                    .chat_store
                    .messages_after(chat_id, boundary_id, self.history_len)
                    .await?;

                let mut history = vec![OllamaService::system_message(format!(
                    "Earlier parts of this conversation were summarized to keep it within \
                     the model's context window. Summary of everything before this point:\n\n{summary}"
                ))];
                history.extend(recent.into_iter().rev().map(Self::to_ollama_message));
                Ok(history)
            }
            _ => {
                let (history, _) = self.chat_store.messages(chat_id, self.history_len, 0).await?;
                Ok(history.into_iter().rev().map(Self::to_ollama_message).collect())
            }
        }
    }

    /// Checks whether the turn that just finished pushed prompt usage over
    /// `compaction_trigger_tokens` and, if so, compacts older history into
    /// `Chat::summary` before returning — so the *next* request (a fresh turn, or
    /// another `continue_chat` later in the same tool-calling round) builds a smaller
    /// prompt via `ollama_history`. Best-effort: a failure here doesn't fail the turn
    /// that already succeeded, it just means history stays as big as it is and gets
    /// another chance to trigger this again.
    async fn maybe_compact(&self, chat_id: i64, prompt_eval_count: Option<u64>) {
        if prompt_eval_count.unwrap_or(0) < self.compaction_trigger_tokens {
            return;
        }

        tracing::info!(
            chat_id,
            prompt_eval_count,
            trigger_threshold = self.compaction_trigger_tokens,
            "compaction triggered for chat_id {chat_id}"
        );

        if let Err(e) = self.compact(chat_id).await {
            tracing::warn!(
                "history compaction failed for chat {chat_id}: {}",
                e.message.as_deref().unwrap_or("unknown error")
            );
        }
    }

    /// Folds the oldest not-yet-summarized messages into `Chat::summary` until what's
    /// left is under `compaction_keep_chars`, merging in the existing summary (if any)
    /// rather than discarding it. No-ops if everything already fits — that means
    /// `compaction_trigger_tokens` fired on a single outsized turn rather than a long
    /// history, which folding can't help with.
    async fn compact(&self, chat_id: i64) -> Result<(), ErrorService> {
        let chat = self.chat_store.chat(chat_id).await?;
        let after_id = chat.summary_up_to_message_id.unwrap_or(0);

        let mut messages = self
            .chat_store
            .messages_after(chat_id, after_id, self.history_len)
            .await?;
        messages.reverse(); // oldest first, easier to reason about a boundary over

        let sizes: Vec<usize> = messages
            .iter()
            .map(|message| message.content.len() + message.thinking.as_deref().map_or(0, str::len))
            .collect();
        let split_at = pick_compaction_boundary(&sizes, self.compaction_keep_chars);

        if split_at == 0 {
            return Ok(());
        }

        let to_fold = &messages[..split_at];
        let new_boundary_id = to_fold.last().map(|m| m.id).unwrap_or(after_id);

        tracing::info!(
            chat_id,
            messages_folded = to_fold.len(),
            had_prior_summary = chat.summary.is_some(),
            "calling summarize for chat_id {chat_id}"
        );

        let summary = self.summarize(chat.summary, to_fold).await?;
        self.chat_store.set_summary(chat_id, summary, new_boundary_id).await?;

        tracing::info!(chat_id, new_boundary_id, "compaction finished for chat_id {chat_id}");

        Ok(())
    }

    /// Produces an updated summary covering `existing_summary` (if any) plus every
    /// message in `to_fold`, via a plain (no tools) Ollama call — not part of the
    /// visible conversation, so it doesn't go through `advance`/get persisted as a chat
    /// message itself.
    async fn summarize(&self, existing_summary: Option<String>, to_fold: &[Message]) -> Result<String, ErrorService> {
        let transcript = to_fold
            .iter()
            .map(|message| match message.role.as_str() {
                "tool" => format!(
                    "[tool result — {}]: {}",
                    message.tool_name.as_deref().unwrap_or("?"),
                    message.content
                ),
                role => format!("[{role}]: {}", message.content),
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prior = existing_summary
            .map(|summary| format!("Summary of everything before this excerpt:\n{summary}\n\n"))
            .unwrap_or_default();

        let system = OllamaService::system_message(
            "Summarize the conversation excerpt that follows into short but complete notes \
             for continuing the conversation later — what was asked, what was done, what was \
             learned, and any specific details (file paths, decisions, exact names) later \
             turns might still need. If a prior summary is included, fold it in rather than \
             dropping it. Pay special attention to detail that's easy to accidentally \
             paraphrase away but matters a lot if lost: a tool result marked truncated (a \
             later turn needs to know it only saw part of something, not the whole thing), \
             and exact code/text snippets that a future edit might need to reproduce \
             verbatim — summarize the surrounding narrative, but don't rewrite exact text \
             like that into your own words. Write plain notes, not a reply — this output \
             replaces the excerpt in the conversation's history, nobody sees it directly."
                .to_string(),
        );
        let user = OllamaService::user_message(format!(
            "{prior}Conversation excerpt to summarize:\n\n{transcript}"
        ));

        // `think: false` — measured head-to-head against the same real fold-candidate
        // messages (`summarize_bench`, since deleted): thinking cost ~2x the time and
        // token budget, and produced a *shorter, less detailed* final summary — the
        // deliberation ate the token budget that would've otherwise gone into exact
        // struct/field names and per-tool specifics, which is exactly what the system
        // prompt above asks it to preserve. Reasoning first turned out to hurt the
        // thing it was meant to help here, not just cost more.
        let response = self.ollama.chat(vec![system], Some(user), &[], Some(false)).await?;
        Ok(response.message.content)
    }

    /// The tool calls the model has asked for that haven't been run yet, without
    /// actually running them — lets a caller check each one's `permission` (and warn
    /// about a `Denied` one) before committing to `use_tool`.
    pub async fn can_use_tool(&self, chat_id: i64) -> Result<CanUseTool, ErrorService> {
        let pending = self.pending_tool_calls(chat_id).await?;

        let mut tools = Vec::with_capacity(pending.len());
        for call in pending {
            tools.push(self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?);
        }

        Ok(CanUseTool {
            can_use: !tools.is_empty(),
            tools,
        })
    }

    /// Runs the next pending tool call (in the order the model requested them) and
    /// persists its result as a `tool`-role message, whether it succeeded, failed, or
    /// was denied — the model needs to see all three outcomes to react sensibly on its
    /// next turn, not just a silent gap. `scope`, if given, overrides whatever's
    /// already stored for this chat/tool for this one call only — it's never
    /// persisted (`allow_scope` is the only thing that persists a grant) — and doubles
    /// as the caller's confirmation that it knows what it's asking for. With no
    /// override, whatever's already stored (if anything) is used instead.
    ///
    /// A call the tool doesn't permit — `ToolPermission::Denied`, whether from no
    /// scope being available at all or from a given/stored one not covering it — never
    /// reaches `call_tool`. It's recorded the same way an execution failure is
    /// (`success: false`, persisted as the `tool` message), but with `denied: true` so
    /// a caller can tell "this needs permission" apart from "this tool actually broke"
    /// without parsing `err`'s text.
    ///
    /// Errors only when there's nothing pending to run, which means the caller didn't
    /// check `can_use_tool` first. Also reports the tool calls still left after this one,
    /// same as `can_use_tool` would, so a caller can tell whether to run another `use_tool`
    /// or move on without a separate round trip.
    pub async fn use_tool(&self, chat_id: i64, scope: Option<Value>) -> Result<UseToolOut, ErrorService> {
        let mut pending = self.pending_tool_calls(chat_id).await?.into_iter();
        let next = pending
            .next()
            .ok_or_else(|| ErrorService::new(StatusCode::BAD_REQUEST, "no pending tool call to run"))?;

        let had_scope;
        let effective_scope = match scope {
            Some(scope) => {
                had_scope = true;
                Some(scope)
            }
            None => {
                let stored = self.stored_scope(chat_id, &next.tool_name).await?;
                had_scope = stored.is_some();
                stored
            }
        };

        let permission = self.tool_permission(&next.tool_name, next.arguments.clone(), effective_scope);

        let (success, denied, err, content) = match permission {
            AgentToolPermission::Allowed => match self.tools.call_tool(&next.tool_name, next.arguments).await {
                Ok(value) => (true, false, None, value),
                Err(e) => {
                    let message = e.to_string();
                    (false, false, Some(message.clone()), Value::String(message))
                }
            },
            AgentToolPermission::Denied { reason, escalation } => {
                // Worded so the model doesn't read one declined call as a ban on the
                // tool as a whole — it's scoped to this specific call, and retrying
                // (same arguments once the user grants it, or different arguments
                // that aren't restricted) is the expected next step, not something to
                // refuse on principle.
                let message = match (had_scope, escalation.is_some()) {
                    (_, false) => format!(
                        "Tool call blocked — '{}' can't be approved for these exact arguments ({reason}). \
                         This only concerns this specific call, not the tool as a whole.",
                        next.tool_name
                    ),
                    (true, true) => format!(
                        "Tool call denied — the permission already granted doesn't cover these arguments \
                         ({reason}). Call it again with arguments the user's willing to approve, or let them \
                         decide."
                    ),
                    (false, true) => format!(
                        "Tool call declined — '{}' hasn't been granted permission in this chat yet ({reason}). \
                         If the user wants to proceed, call it again; they'll be asked to approve it then.",
                        next.tool_name
                    ),
                };
                (false, true, Some(message.clone()), Value::String(message))
            }
        };

        let stored = self
            .chat_store
            .new_message(NewMessage {
                chat_id,
                role: "tool".to_string(),
                content: content.to_string(),
                tool_name: Some(next.tool_name.clone()),
                thinking: None,
                thought_duration_ms: None,
                tool_success: Some(success),
                tool_denied: denied,
                tool_calls: vec![],
            })
            .await?;

        let mut tools = Vec::with_capacity(pending.len());
        for call in pending {
            tools.push(self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?);
        }

        Ok(UseToolOut {
            success,
            denied,
            tool_name: next.tool_name,
            err,
            content,
            created_at: stored.created_at,
            tools,
        })
    }

    /// Persists a scope grant for a tool within a chat, so future calls to that tool in
    /// that chat can be `Allowed` without asking again. Only checks that `tool_name`
    /// refers to a real registered tool before storing — `scope`'s actual shape is
    /// opaque to everything except the tool that offered it (see `ScopeGrant`), so
    /// there's nothing else here that could meaningfully be validated ahead of time.
    pub async fn allow_scope(&self, chat_id: i64, tool_name: String, scope: Value) -> Result<(), ErrorService> {
        if self.tools.get_tool(&tool_name).is_none() {
            return Err(ErrorService::new(
                StatusCode::BAD_REQUEST,
                format!("no tool named '{tool_name}'"),
            ));
        }

        self.permission_store.update_scope(chat_id, &tool_name, scope).await?;

        Ok(())
    }

    /// The scope already granted to a tool within a chat, if any — `None` rather than
    /// an error when nothing's been granted yet, since that's the normal/expected case
    /// for most tool calls, not a failure.
    async fn stored_scope(&self, chat_id: i64, tool_name: &str) -> Result<Option<Value>, ErrorService> {
        match self.permission_store.get_scope(chat_id, tool_name).await {
            Ok(scope) => Ok(Some(scope)),
            Err(PermissionStoreErrors::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Resolves a tool call's permission into the facade-facing shape: an unknown tool
    /// name and a `ToolSerializationError` (couldn't even read `data`) both collapse
    /// into `Denied` with no escalation — from a caller's perspective both just mean
    /// "this can't run as given," the distinction between them only matters to
    /// whoever's implementing the tool. A `Denied` from the tool itself always carries
    /// its `reason` through, whether or not it came with an `escalation` — a hard
    /// refusal still needs to reach the UI and the model, not just silently vanish.
    fn tool_permission(&self, tool_name: &str, data: Value, scope: Option<Value>) -> AgentToolPermission {
        let Some(tool) = self.tools.get_tool(tool_name) else {
            return AgentToolPermission::Denied {
                reason: format!("no tool named '{tool_name}'"),
                escalation: None,
            };
        };

        match tool.is_dangerous(data, scope) {
            Ok(ToolPermission::Allowed) => AgentToolPermission::Allowed,
            Ok(ToolPermission::Denied { reason, escalation }) => AgentToolPermission::Denied {
                reason,
                escalation: escalation.map(|grant| AgentScopeGrant {
                    scope: grant.scope,
                    ui_message: grant.ui_message,
                }),
            },
            Err(e) => AgentToolPermission::Denied {
                reason: format!("couldn't validate call arguments: {e}"),
                escalation: None,
            },
        }
    }

    /// Finds tool calls the model has requested but that don't have a result message
    /// yet, by walking the chat's messages newest-first: skip past `tool` rows (already
    /// resolved), and whatever comes right after them decides the answer. If that's an
    /// `assistant` message with `tool_calls`, the ones beyond however many `tool` rows
    /// we just skipped are still pending (`tool_calls` is id-ordered, i.e. the order the
    /// model requested them in). Anything else — a plain assistant reply, a `user`
    /// message, or an empty chat — means nothing is pending; in particular a fresh
    /// `user` message always wins even if older unresolved tool calls sit further back,
    /// since the user talking again supersedes them.
    async fn pending_tool_calls(&self, chat_id: i64) -> Result<Vec<ToolCallOut>, ErrorService> {
        let (messages, _) = self.chat_store.messages(chat_id, self.history_len, 0).await?;

        let resolved = messages.iter().take_while(|message| message.role == "tool").count();

        let Some(candidate) = messages.get(resolved) else {
            return Ok(vec![]);
        };

        if candidate.role != "assistant" {
            return Ok(vec![]);
        }

        Ok(candidate.tool_calls[resolved.min(candidate.tool_calls.len())..].to_vec())
    }

    /// Builds the facade-facing view of a tool call the model has requested, including
    /// whether it's actually permitted right now — always checked against whatever
    /// scope is already stored for this chat/tool (never a one-time override; only
    /// `use_tool` accepts one of those), since this is a preview, not a commitment to
    /// run anything.
    async fn to_agent_tool_call(
        &self,
        chat_id: i64,
        name: String,
        arguments: Value,
    ) -> Result<AgentToolCall, ErrorService> {
        let scope = self.stored_scope(chat_id, &name).await?;
        let permission = self.tool_permission(&name, arguments.clone(), scope);

        Ok(AgentToolCall {
            permission,
            name,
            arguments,
        })
    }

    /// Maps a persisted `Message` back into the shape Ollama's `/api/chat` expects for
    /// history. `OllamaToolCall::id` is left empty — we never persisted Ollama's
    /// original per-call id (only `function.name`/`arguments`, which is all replaying
    /// history needs), and it's not yet confirmed whether Ollama expects/uses `id` at
    /// all on the *outgoing* (request) side versus just returning it in responses.
    fn to_ollama_message(message: Message) -> OllamaChatMessage {
        let tool_calls: Vec<OllamaToolCall> = message
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(index, call)| OllamaToolCall {
                id: String::new(),
                function: OllamaToolCallFunction {
                    index: Some(index as u32),
                    name: call.tool_name,
                    arguments: call.arguments,
                },
            })
            .collect();

        OllamaChatMessage {
            role: message.role,
            content: message.content,
            tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            tool_name: message.tool_name,
            thinking: None,
        }
    }
}

/// `Agent`'s outputs (this one included) derive `Serialize` and go straight out as JSON
/// from route handlers, unlike service-layer types — `Agent` *is* the app's public API
/// shape already (that's what a facade is), so there's nothing upstream-specific left to
/// strip before a route can return it, unlike `OllamaChatMessage` et al.
#[derive(Serialize, ToSchema)]
pub struct AgentToolCall {
    pub permission: AgentToolPermission,
    pub name: String,
    #[schema(value_type = Object)]
    pub arguments: Value,
}

/// Facade-owned mirror of `tools::base::ToolPermission` — kept as a separate type
/// (rather than deriving `Serialize`/`ToSchema` on the original and reusing it
/// directly) for the same reason `OllamaChatMessage` never goes straight out over our
/// API: the tools layer's internal shape shouldn't be what callers of `Agent` end up
/// depending on.
#[derive(Serialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentToolPermission {
    Allowed,
    Denied {
        reason: String,
        escalation: Option<AgentScopeGrant>,
    },
}

/// Facade-owned mirror of `tools::base::ScopeGrant` — see `AgentToolPermission` for
/// why this isn't the original type reused directly.
#[derive(Serialize, ToSchema)]
pub struct AgentScopeGrant {
    #[schema(value_type = Object)]
    pub scope: Value,
    pub ui_message: String,
}

#[derive(Serialize, ToSchema)]
pub struct CanUseTool {
    pub can_use: bool,
    pub tools: Vec<AgentToolCall>,
}

#[derive(Serialize, ToSchema)]
pub struct ChatOut {
    pub content: String,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
    pub can_use_tools: bool,
    pub tool_calls: Vec<AgentToolCall>,
    /// The model's reasoning trace for this reply, when `think` was requested and the
    /// model produced one.
    pub thinking: Option<String>,
    /// How long the Ollama call for this reply took, in milliseconds — see
    /// `NewMessage::thought_duration_ms` for what this does and doesn't measure. Always
    /// set (unlike the same-named field on `Message`/`NewMessage`) — `advance` times
    /// every call it makes, there's no path through it that skips this.
    pub thought_duration_ms: i64,
}

#[derive(Serialize, ToSchema)]
pub struct UseToolOut {
    pub success: bool,
    /// Set when `success` is `false` specifically because the call wasn't permitted
    /// (no/insufficient scope) — distinct from `success: false` with `denied: false`,
    /// which means the tool ran and genuinely failed. Lets a caller (e.g. the UI) tell
    /// the two apart without matching on `err`'s text.
    pub denied: bool,
    pub tool_name: String,
    pub err: Option<String>,
    /// What the tool itself returned — the same value persisted as the `tool` message's
    /// content. On failure this is the error message wrapped as a JSON string, same as
    /// what got persisted.
    #[schema(value_type = Object)]
    pub content: Value,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
    pub tools: Vec<AgentToolCall>,
}
