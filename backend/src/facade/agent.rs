use std::sync::Arc;
use std::time::Instant;

use axum::http::StatusCode;
use sea_orm::prelude::DateTimeUtc;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Semaphore;
use utoipa::ToSchema;

use crate::services::{
    chat_store::{ChatStore, MessageTimings, NewMessage, NewToolCall},
    error::ErrorService,
    event_bus::{EventBus, ServerEvent},
    file_store::FileStore,
    job_store::JobStore,
    llm::{ChatMessage, LlmProviders, ThinkChoice},
    permission_store::PermissionStore,
    preset_store::PresetStore,
    settings_store::SettingsStore,
    tools::ToolService,
};
use crate::facade::launch::LaunchFacade;
use crate::facade::one_shot::OneShot;
use crate::tools::base::{Tool, ToolContext};
use crate::tools::subagent::{self, SubagentHandle};

mod compaction;
mod history;
mod model_call;
mod notices;
mod prompts;
mod subagent_run;
mod tool_calls;

use compaction::Compaction;
use history::History;
use notices::Notices;
use tool_calls::ToolCalls;
use model_call::{ModelCall, ReplyProblem, Regenerations};
use prompts::with_attached_files_note;
pub use prompts::default_system_prompt;



/// rough characters-per-token used to turn that into a size. The result is what the whole run was
/// for, so it's inlined instead of left for a tool call — but a single message bigger than the
/// window can't be compacted away, so a cap is the backstop. It scales with the context length like
/// the compaction thresholds do; the sub-agent's own prompt is what asks it to keep results short.
const INLINED_RESULT_FRACTION: f64 = 0.15;
const INLINED_RESULT_CHARS_PER_TOKEN: f64 = 3.0;

/// Facade over the model providers, `ChatStore`, and `ToolService` — where the actual
/// "fetch history, call Ollama, persist the result, run tool calls" sequencing lives,
/// rather than in route handlers or inside any one of the services it composes. Holds
/// its own `Arc` clones of each rather than borrowing from `AppState`, so it can be
/// used independently of any particular request's `State` extraction.
#[derive(Clone)]
pub struct Agent {
    /// What the model is sent for a chat: the system message and the messages after it.
    history: History,
    /// Provider, launch profile, call parameters and the turn's claim on the model server, and what
    /// is done with a reply that can't be used.
    model: ModelCall,
    /// Keeps the prompt inside the window: clearing old tool results, the model's notes, the fold.
    compaction: Compaction,
    chat_store: Arc<ChatStore>,
    tools: Arc<ToolService>,
    /// The services the agent itself reaches (files, events), and the template `ToolCalls` makes each
    /// tool's real, per-call context from. See `ToolContext`'s own doc comment for why it isn't `AppState`.
    tool_context: ToolContext,
    /// The model's tool calls: which are pending, whether each is permitted, running them.
    tool_calls: ToolCalls,
    /// Finished background jobs and sub-agents, reported to the model as notices.
    notices: Notices,
    /// Per-user settings — consulted once per turn for the user's custom system prompt
    /// (a user without one gets the built-in default instead).
    settings_store: Arc<SettingsStore>,
    /// One permit: sub-agents run one at a time. They all use the same model on the same GPU, and
    /// two of them taking turns would each evict the other's cached prompt on every call — slower
    /// for both than running back to back.
    subagent_slot: Arc<Semaphore>,
}

impl Agent {
    // One parameter per service the agent depends on — it's a wiring point, so it grows
    // with the services rather than with anything that would be clearer grouped.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        providers: LlmProviders,
        chat_store: Arc<ChatStore>,
        tools: Arc<ToolService>,
        file_store: Arc<FileStore>,
        job_store: Arc<JobStore>,
        events: Arc<EventBus>,
        permission_store: Arc<PermissionStore>,
        settings_store: Arc<SettingsStore>,
        presets: Arc<PresetStore>,
        launch: Arc<LaunchFacade>,
        context_length: u64,
    ) -> Self {
        // `chat_id: 0` here is a placeholder — never read as-is, always replaced via
        // `copy_with_chat_id` before a tool actually sees this context. `providers`
        // is cloned (an `Arc` bump) rather than moved directly, since `Agent` itself also
        // holds its own copy below.
        let tool_context =
            ToolContext {
                file_store,
                providers: providers.clone(),
                job_store: job_store.clone(),
                chat_store: chat_store.clone(),
                events,
                chat_id: 0,
                user_id: 0,
                model: String::new(),
                provider: String::new(),
                subagents: Arc::new(SubagentHandle::new()),
            };
        let history = History::new(chat_store.clone(), settings_store.clone());
        let model = ModelCall::new(providers.clone(), presets, launch, context_length);
        let compaction = Compaction::new(
            chat_store.clone(),
            settings_store.clone(),
            tools.clone(),
            model.clone(),
            history.clone(),
            OneShot::new(providers),
        );
        let tool_calls = ToolCalls::new(chat_store.clone(), tools.clone(), permission_store, tool_context.clone());
        let notices = Notices::new(
            chat_store.clone(),
            job_store,
            tool_calls.clone(),
            (context_length as f64 * INLINED_RESULT_FRACTION * INLINED_RESULT_CHARS_PER_TOKEN) as u64,
        );
        Self {
            history,
            model,
            compaction,
            tool_calls,
            chat_store,
            tools,
            tool_context,
            notices,
            settings_store,
            subagent_slot: Arc::new(Semaphore::new(1)),
        }
    }

    /// Persists `prompt` (plus `images`, if any — base64-encoded, no data-URL prefix —
    /// and `file_ids`, if any) as a `user` message, then advances the chat same as
    /// `continue_chat` does. The prompt is saved before the Ollama call, not after, so
    /// a failed/slow Ollama call never loses what the user actually sent. `think` is
    /// forwarded to Ollama as-is — see `OllamaService::chat` for its default.
    /// `file_ids` are only persisted here, never sent to Ollama or otherwise read —
    /// feeding a file's actual content into a turn is a separate, not-yet-built step —
    /// but each one does get claimed for this chat first (`FileStore::attach_to_chat`):
    /// a file can be uploaded before any chat exists to attach it to (the home page's
    /// case, `chat_id: None` until now), and this is the moment it actually becomes
    /// this chat's. A no-op for a file that was already uploaded with a real `chat_id`
    /// (e.g. from an existing chat's own composer) — this just re-sets it to the same
    /// value either way, cheaper than checking first.
    pub async fn chat(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        file_ids: Vec<i64>,
        think: Option<ThinkChoice>,
    ) -> Result<ChatOut, ErrorService> {
        let last_prompt_tokens = self.chat_store.chat(chat_id).await.ok().and_then(|c| c.last_prompt_tokens.map(|t| t as u64));
        self.compaction.maybe_compact(chat_id, last_prompt_tokens, think.clone()).await;
        let messages = self.history.for_chat(chat_id).await?;

        for &file_id in &file_ids {
            self.tool_context.file_store.attach_to_chat(file_id, chat_id).await?;
        }

        let user_message = self
            .chat_store
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
                images: images.clone(),
                file_ids: file_ids.clone(),
                prompt_tokens: None,
                eval_tokens: None,
                timings: MessageTimings::default(),
            })
            .await?;

        let notices = self.notices.flush_job_notices(chat_id).await?;

        // Everything newer than `messages`, oldest first: the user's prompt, then any
        // job notices flushed just above (persisted after it, so this is also their
        // order in the chat). The last of them is what `advance` sends as the newest
        // message; the rest go in front of it as ordinary history.
        let ollama_content = with_attached_files_note(prompt, &file_ids);
        let mut tail = vec![ChatMessage::user_with_images(ollama_content, images)];
        tail.extend(notices.iter().map(|notice| ChatMessage::user(notice.content.clone())));
        let new_message = tail.pop();
        let mut messages = messages;
        messages.extend(tail);

        let mut out = self.advance(chat_id, messages, new_message, think, notices, true).await?;
        out.user_message_id = Some(user_message.id);
        Ok(out)
    }

    /// Sends a chat's existing history to Ollama as-is and persists whatever it replies
    /// with, without adding any new turn first. For continuing after tool results:
    /// `use_tool` already persisted the tool's output as a message, so getting the
    /// model's next response needs nothing more than asking again — no synthetic user
    /// message, no requirement that every pending tool call has been resolved first (a
    /// tool failing is a valid reason to continue too, and forcing the caller through the
    /// rest of an in-flight batch first would just be busywork).
    pub async fn continue_chat(&self, chat_id: i64, think: Option<ThinkChoice>) -> Result<ChatOut, ErrorService> {
        let last_prompt_tokens = self.chat_store.chat(chat_id).await.ok().and_then(|c| c.last_prompt_tokens.map(|t| t as u64));
        self.compaction.maybe_compact(chat_id, last_prompt_tokens, think.clone()).await;
        let notices = self.notices.flush_job_notices(chat_id).await?;
        let messages = self.history.for_chat(chat_id).await?;
        self.advance(chat_id, messages, None, think, notices, true).await
    }

    /// Reports the background jobs (and sub-agents) that finished since the model was last told about
    /// one, without calling the model — see `Notices::flush_notices`.
    pub async fn flush_notices(&self, chat_id: i64) -> Result<Vec<NoticeOut>, ErrorService> {
        self.notices.flush_notices(chat_id).await
    }

    /// Has the model answer again where it answered last, and replaces that answer with the new
    /// one. Only a plain final reply qualifies: the chat's newest message, from the assistant,
    /// with no tool calls, straight after the user's own message (so no tool ran in between, which
    /// would be run again or answered differently) and newer than the compaction boundary (an
    /// older one is no longer part of the history the model is sent), in an ordinary chat — not a
    /// sub-agent's and not a messaging plugin's. `message_id` is the reply
    /// the caller is looking at, so a stale view of the chat can't replace a different message.
    ///
    /// The old reply is deleted only after the new one is stored: a model that is down or fails
    /// leaves the chat as it was. The history sent is the stored one minus that reply, and job
    /// notices are left for the next turn to flush — one flushed now would land between the old
    /// and the new reply.
    pub async fn regenerate(&self, chat_id: i64, message_id: i64, think: Option<ThinkChoice>) -> Result<ChatOut, ErrorService> {
        let not_regenerable = |why: &str| ErrorService::new(StatusCode::CONFLICT, format!("that reply can't be regenerated: {why}"));

        let chat = self.chat_store.chat(chat_id).await?;
        // A sub-agent's chat is driven by the backend, and a plugin's reply has already gone out to
        // the messaging app, where a replacement here would never arrive.
        if chat.parent_chat_id.is_some() {
            return Err(not_regenerable("a sub-agent's chat runs on its own"));
        }
        if self.chat_store.is_plugin_chat(chat_id).await? {
            return Err(not_regenerable("its reply has already been sent to a messaging app"));
        }
        let (newest, _) = self.chat_store.messages(chat_id, 2, 0).await?;
        let [reply, before] = newest.as_slice() else {
            return Err(not_regenerable("it isn't a reply to a message"));
        };
        if reply.id != message_id {
            return Err(not_regenerable("it isn't the newest message"));
        }
        if reply.role != "assistant" || !reply.tool_calls.is_empty() {
            return Err(not_regenerable("only a plain reply can be"));
        }
        if before.role != "user" {
            return Err(not_regenerable("it doesn't directly follow a message of yours"));
        }
        if chat.summary_up_to_message_id.is_some_and(|boundary| reply.id <= boundary) {
            return Err(not_regenerable("it is already folded into the chat's summary"));
        }

        let mut messages = self.history.for_chat(chat_id).await?;
        messages.pop();

        let out = self.advance(chat_id, messages, None, think, vec![], true).await?;
        self.chat_store.delete_message(chat_id, message_id).await?;
        Ok(out)
    }

    /// Sends `messages` (plus `new_message`, if any) to Ollama, prefixed with
    /// `SYSTEM_PROMPT`, and persists whatever it replied with as an assistant message,
    /// carrying `tool_calls` if the model requested any. Shared by `chat` and
    /// `continue_chat`, which differ only in whether there's a new turn to add before
    /// asking the model to respond. `think` is forwarded to Ollama as-is (defaulted
    /// there, not here). `SYSTEM_PROMPT` is prepended fresh on every call rather than
    /// stored in `chat_store`, so it can be changed without touching existing chats'
    /// history. `thought_duration_ms` times the whole Ollama call, not just the
    /// `<think>` portion — see its doc comment on `NewMessage` for why — and includes any
    /// regenerations (`unusable_reply`), since that's how long the reply really took. `notices` are
    /// job notices the caller already persisted just before this call (see
    /// `flush_job_notices`), carried through into the returned `ChatOut` so a client can
    /// show them ahead of the reply.
    async fn advance(
        &self,
        chat_id: i64,
        messages: Vec<ChatMessage>,
        new_message: Option<ChatMessage>,
        think: Option<ThinkChoice>,
        notices: Vec<NoticeOut>,
        can_auto_continue: bool,
    ) -> Result<ChatOut, ErrorService> {
        let result = self.advance_once(chat_id, messages, new_message, think, notices, can_auto_continue).await;
        // A reply that asks for tools means the turn goes on (the tools run, the model is called
        // again): the model server stays claimed. Anything else ends it.
        if !result.as_ref().is_ok_and(|out| out.can_use_tools) {
            self.model.release(chat_id);
        }
        result
    }

    async fn advance_once(
        &self,
        chat_id: i64,
        mut messages: Vec<ChatMessage>,
        new_message: Option<ChatMessage>,
        think: Option<ThinkChoice>,
        notices: Vec<NoticeOut>,
        can_auto_continue: bool,
    ) -> Result<ChatOut, ErrorService> {
        // Snapshot the current tool set once per turn. Each element is an Arc<dyn Tool>
        // that can be held past any .await without keeping the ToolService lock open,
        // so plugin enable/disable can update the set concurrently with ongoing turns.
        let tools_snapshot: Vec<Arc<dyn Tool>> = self.tools.snapshot_tools().await;

        // The chat metadata, read fresh so a model switch takes effect with the next prompt and
        // last_prompt_tokens is available for accurate budget calculation.
        let mut chat = self.chat_store.chat(chat_id).await?;
        self.model.bind(&mut chat);

        // Which tools the model is shown depends on whether this chat is a sub-agent's — see
        // `subagent::available_to`. Filtering the list (rather than only refusing a call) is what
        // keeps a sub-agent from being able to try starting one of its own.
        let is_subagent = chat.parent_chat_id.is_some();
        let tools: Vec<&dyn Tool> = tools_snapshot
            .iter()
            .map(|t| t.as_ref())
            .filter(|t| subagent::available_to(t.function_name(), is_subagent))
            .collect();
        let provider = self.model.provider(&chat)?;
        let params = self.model.params(&chat).await?;
        self.model.hold(&chat, provider.as_ref(), params.launch.as_ref()).await?;
        let model = chat.model.clone();
        let known_prompt_tokens = chat.last_prompt_tokens.map(|t| t as u64);

        // `History::for_chat` leads with its own system message (the compaction summary)
        // once a chat has one — folded into this same system message rather than sent
        // as a second one, since some chat templates (e.g. Qwen's) reject more than one
        // system-role message anywhere but position 0 ("System message must be at the
        // beginning"). Deliberately holds no per-call-volatile content (no timestamp —
        // see `now_note` below for why) so it stays byte-identical across a chat's
        // turns except when a compaction fold actually changes the summary — that's
        // what lets Ollama/llama.cpp's prompt cache match this prefix and reuse it
        // instead of reprocessing the whole history on every single turn.
        // The user's own system prompt, if they set one — fetched once per turn, the same
        // as the chat row above, so a change (or reset) takes effect from the next turn,
        // exactly like a model switch. A user without one gets the built-in default.
        let system_prompt = self.history.system_prompt(&chat, &mut messages).await?;

        // Collected now, while `messages` still holds this turn's history, so it
        // survives the `extend` below. Only actually used once we know this response
        // has no further tool calls of its own — see the `stored` message below.
        let attached_files = History::attached_files(&messages);
        let streak_note = History::streak_notice(&messages);

        let mut messages_with_system = vec![ChatMessage::system(system_prompt)];
        messages_with_system.extend(messages);

        // Stamped onto whichever message is newest — `new_message` when there is one
        // (`chat`'s fresh-turn path), otherwise the last entry already in
        // `messages_with_system` (`continue_chat`'s path, where that's the tool result
        // `use_tool` just persisted). Either way that's content this request sends to
        // Ollama for the first time, so appending it here doesn't cost any additional
        // prompt-cache reuse — unlike baking it into `system_prompt` above (the old
        // approach), which changed every single call and, being the prompt's very
        // first tokens, invalidated the *entire* cached prefix on every turn (see
        // `SYSTEM_PROMPT`'s own rule for the paired instruction — it now points here
        // instead of claiming a fixed position). Computed fresh each call so it's
        // never stale, same as before; still reaches every `Agent` instance including
        // the messaging-plugin one (empty `ToolService`, no `os.get_date` to fall back
        // on).
        let now_note = format!(
            "\n\n[Current real date and time (UTC): {}]",
            chrono::Utc::now().format("%A, %B %-d, %Y %H:%M:%S UTC")
        );
        let new_message = match new_message {
            Some(mut msg) => {
                msg.content.push_str(&now_note);
                if let Some(ref note) = streak_note {
                    msg.content.push_str(note);
                }
                Some(msg)
            }
            None => {
                if let Some(last) = messages_with_system.last_mut() {
                    last.content.push_str(&now_note);
                    if let Some(ref note) = streak_note {
                        last.content.push_str(note);
                    }
                }
                None
            }
        };

        let started_at = Instant::now();
        let mut regenerations = Regenerations::default();
        let response = loop {
            let response = provider
                .chat(
                    messages_with_system.clone(),
                    new_message.clone(),
                    &tools,
                    think.clone(),
                    &model,
                    known_prompt_tokens,
                    &params,
                )
                .await?;

            // The live "thinking" indicator's spend hint (see `ServerEvent::TurnProgress`):
            // model calls are non-streaming, so a token count exists only at the moment one
            // returns — publish it there, including for regenerated (unusable) replies,
            // since their tokens were spent too.
            if let Some(eval_tokens) = response.eval_count() {
                self.tool_context.events.publish(ServerEvent::TurnProgress {
                    chat_id,
                    eval_tokens,
                    prompt_tokens: response.prompt_eval_count(),
                });
            }

            let Some(problem) = ModelCall::unusable_reply(provider.as_ref(), &response, &model).await else { break response };

            if problem == ReplyProblem::CutOffInThinking && can_auto_continue {
                let thought_trace = response
                    .message
                    .thinking
                    .as_deref()
                    .unwrap_or(&response.message.content)
                    .trim()
                    .to_string();
                let thought_duration_ms = i64::try_from(started_at.elapsed().as_millis()).unwrap_or(i64::MAX);
                let prompt_eval_count = response.prompt_eval_count();
                let eval_count = response.eval_count();
                let timings = ModelCall::timings_of(&response);

                tracing::warn!(
                    chat_id,
                    "model cut off in thinking; storing thought process as message and triggering automatic continuation"
                );

                // Store cut thoughts as message content (not as thinking) with an explicit continuation
                // marker so the model on the continuation turn knows it was interrupted mid-thought.
                let content = prompts::cut_off_thoughts_message(&thought_trace);
                self.chat_store
                    .new_message(NewMessage {
                        chat_id,
                        role: "assistant".to_string(),
                        content,
                        tool_name: None,
                        thinking: None,
                        thought_duration_ms: Some(thought_duration_ms),
                        tool_success: None,
                        tool_denied: false,
                        tool_calls: vec![],
                        images: vec![],
                        file_ids: vec![],
                        prompt_tokens: prompt_eval_count.map(|c| c as i64),
                        eval_tokens: eval_count.map(|c| c as i64),
                        timings,
                    })
                    .await?;

                let total_tokens = prompt_eval_count.map(|pt| pt + eval_count.unwrap_or(0));
                if let Some(total) = total_tokens {
                    if let Err(e) = self.chat_store.set_last_prompt_tokens(chat_id, Some(total as i64)).await {
                        tracing::warn!(chat_id, "failed to update last_prompt_tokens: {e:?}");
                    }
                }

                // Check compaction against total tokens (prompt + generated thoughts), not just prompt_eval_count,
                // so compaction frees headroom BEFORE the recursive continuation turn if context is full.
                self.compaction.maybe_compact(chat_id, total_tokens, think.clone()).await;

                // Persist the continuation prompt into the chat store so the database message
                // sequence is strictly alternating (assistant -> notice -> assistant),
                // preventing Ollama's "Cannot have 2 or more assistant messages at the end" error.
                // `notice` (not `user`) so this renders as the same muted, backend-written marker a
                // finished-job notice does, not a fake chat bubble the user never actually typed —
                // `History::to_message` already sends any `notice` to Ollama as a `user` turn either way.
                let continuation_text = prompts::CUT_OFF_CONTINUATION.to_string();
                self.chat_store
                    .new_message(NewMessage {
                        chat_id,
                        role: "notice".to_string(),
                        content: continuation_text,
                        tool_name: None,
                        thinking: None,
                        thought_duration_ms: None,
                        tool_success: None,
                        tool_denied: false,
                        tool_calls: vec![],
                        images: vec![],
                        file_ids: vec![],
                        prompt_tokens: None,
                        eval_tokens: None,
                        timings: MessageTimings::default(),
                    })
                    .await?;

                let fresh_messages = self.history.for_chat(chat_id).await?;
                return Box::pin(self.advance(chat_id, fresh_messages, None, think, notices, false)).await;
            }

            if !regenerations.allow(problem) {
                tracing::warn!(chat_id, problem = problem.describe(), "model reply unusable, keeping it anyway");
                break response;
            }

            let thinking_tail: String = response
                .message
                .thinking
                .as_deref()
                .unwrap_or_default()
                .chars()
                .rev()
                .take(160)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            tracing::warn!(chat_id, problem = problem.describe(), %thinking_tail, "model reply unusable, regenerating");
        };
        let thought_duration_ms = i64::try_from(started_at.elapsed().as_millis()).unwrap_or(i64::MAX);
        let prompt_eval_count = response.prompt_eval_count();
        let eval_count = response.eval_count();
        let prompt_tokens = prompt_eval_count.map(|c| c as i64);
        let eval_tokens = eval_count.map(|c| c as i64);
        let timings = ModelCall::timings_of(&response);

        let thinking = response.message.thinking.clone();

        let requested_tool_calls = response.message.tool_calls.unwrap_or_default();
        let new_tool_calls: Vec<NewToolCall> = requested_tool_calls
            .iter()
            .map(|call| NewToolCall {
                tool_name: call.function.name.clone(),
                arguments: call.function.arguments.clone(),
            })
            .collect();

        // `ui.attach_file` results only ever land on the turn's *final* reply — the
        // message the model actually prints once it's done calling tools — never on
        // an intermediate tool-calling message, which usually has no real content of
        // its own for a file to visibly hang off of.
        let file_ids = if new_tool_calls.is_empty() { attached_files } else { vec![] };

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
                images: vec![],
                file_ids,
                prompt_tokens,
                eval_tokens,
                timings,
            })
            .await?;

        if let Some(pt) = prompt_eval_count {
            let total = pt + eval_count.unwrap_or(0);
            if let Err(e) = self.chat_store.set_last_prompt_tokens(chat_id, Some(total as i64)).await {
                tracing::warn!(chat_id, "failed to update last_prompt_tokens: {e:?}");
            }
        }

        let mut tool_calls = Vec::with_capacity(requested_tool_calls.len());
        for call in requested_tool_calls {
            tool_calls.push(
                self.tool_calls.to_agent_tool_call(chat_id, call.function.name, call.function.arguments)
                    .await?,
            );
        }

        let out = ChatOut {
            id: stored.id,
            content: stored.content,
            created_at: stored.created_at,
            can_use_tools: !tool_calls.is_empty(),
            tool_calls,
            thinking,
            thought_duration_ms,
            file_ids: stored.file_ids,
            notices,
            eval_tokens,
            prompt_tokens,
            user_message_id: None,
        };

        let total_tokens = prompt_eval_count.map(|pt| pt + eval_count.unwrap_or(0));
        self.compaction.maybe_compact(chat_id, total_tokens, think.clone()).await;

        Ok(out)
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
        self.tool_calls.run_next_tool(chat_id, scope, false).await
    }

    /// The tool calls the model has asked for that haven't been run yet, without
    /// actually running them — lets a caller check each one's `permission` (and warn
    /// about a `Denied` one) before committing to `use_tool`.
    pub async fn can_use_tool(&self, chat_id: i64) -> Result<CanUseTool, ErrorService> {
        self.tool_calls.can_use(chat_id).await
    }

    /// Persists a scope grant for a tool within a chat, so future calls to that tool (or any other
    /// tool sharing one of its buckets) can be `Allowed` without asking again.
    pub async fn allow_scope(&self, chat_id: i64, tool_name: String, scope: Value) -> Result<(), ErrorService> {
        self.tool_calls.allow_scope(chat_id, tool_name, scope).await
    }
}

/// `Agent`'s outputs (this one included) derive `Serialize` and go straight out as JSON
/// from route handlers, unlike service-layer types — `Agent` *is* the app's public API
/// shape already (that's what a facade is), so there's nothing upstream-specific left to
/// strip before a route can return it, unlike `ChatMessage` et al.
#[derive(Serialize, ToSchema)]
pub struct AgentToolCall {
    pub permission: AgentToolPermission,
    pub name: String,
    #[schema(value_type = Object)]
    pub arguments: Value,
}

/// Facade-owned mirror of `tools::base::ToolPermission` — kept as a separate type
/// (rather than deriving `Serialize`/`ToSchema` on the original and reusing it
/// directly) for the same reason `ChatMessage` never goes straight out over our
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
    /// The persisted assistant message's own id — lets a client that already has this
    /// reply (from the live response, before any reload) key UI state against it (e.g.
    /// which messages have their thinking trace expanded) the same way it would for a
    /// message that came back from `GET /chats/messages`, instead of that state being a
    /// no-op until the next full fetch assigns a real id.
    pub id: i64,
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
    /// Mirrors `NewMessage::file_ids` for this reply — a `ui.attach_file` call earlier
    /// in the same turn, resolved onto this (the turn's final, non-tool-calling) reply.
    /// Without this, a freshly-arrived reply couldn't show its attachment until the
    /// chat was reloaded from `GET /chats/messages`, which is the only other place
    /// `file_ids` comes from.
    pub file_ids: Vec<i64>,
    /// Background-job notices persisted just before this reply, oldest first — messages
    /// the client should show in the chat ahead of `content`, in this order. Empty
    /// unless a job finished since the previous turn.
    pub notices: Vec<NoticeOut>,
    /// This reply's own Ollama `eval_count` — how many tokens generating it cost. `null`
    /// only when Ollama didn't report one (older Ollama without `num_ctx`/metrics).
    /// Mirrors the `eval_tokens` column persisted with the same message.
    pub eval_tokens: Option<i64>,
    /// The prompt size Ollama measured for this reply's call — the running context
    /// usage right after this reply was generated. Same source as `chats.last_prompt_tokens`.
    pub prompt_tokens: Option<i64>,
    /// The id the user's own message was stored under — set only on the reply to `chat`, which is
    /// the one call that adds a message of the user's. A client that showed that message before
    /// the reply came back has no id for it yet, and needs one to act on it (edit, delete).
    pub user_message_id: Option<i64>,
}

/// One `notice` message: the backend telling the chat a background job ended.
#[derive(Serialize, ToSchema)]
pub struct NoticeOut {
    pub content: String,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
}

#[derive(Serialize, ToSchema)]
pub struct UseToolOut {
    /// The persisted `tool` message's own id — same reason as `ChatOut::id`: lets a
    /// client key UI state (whether this call's output is expanded) against a message
    /// it only has from the live response, without waiting for a reload to assign one.
    pub id: i64,
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

