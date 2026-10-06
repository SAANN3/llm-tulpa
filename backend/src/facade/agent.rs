use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::http::StatusCode;
use sea_orm::prelude::DateTimeUtc;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::Semaphore;
use utoipa::ToSchema;

use crate::services::{
    chat_store::{ChatStore, MessageTimings, NewMessage, NewToolCall, ToolCallOut},
    error::ErrorService,
    event_bus::{EventBus, ServerEvent},
    file_store::FileStore,
    job_store::{JobKind, JobRecord, JobStatus, JobStore},
    llm::{ChatMessage, LlmProviders, ThinkChoice},
    permission_store::{PermissionStore, PermissionStoreErrors},
    preset_store::PresetStore,
    settings_store::SettingsStore,
    tools::ToolService,
};
use crate::facade::launch::LaunchFacade;
use crate::facade::one_shot::OneShot;
use crate::tools::base::{ResolvedScope, Tool, ToolContext, ToolPermission};
use crate::tools::subagent::{self, SubagentHandle};

mod compaction;
mod history;
mod model_call;
mod prompts;
mod subagent_run;

use compaction::Compaction;
use history::History;
use model_call::{ModelCall, ReplyProblem, Regenerations};
use prompts::{command_preview, job_notice_text, with_attached_files_note, SubagentEnd, INTERRUPTED_TOOL_MESSAGE};
pub use prompts::default_system_prompt;


/// How many of a chat's newest messages `pending_tool_calls` looks through: it walks back over the
/// tool results of the last reply, a handful at most.
const PENDING_TOOL_CALLS_LOOKBACK: u64 = 500;


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
    /// Template `use_tool` calls `copy_with_chat_id` on to get the real, per-call
    /// context — its own `chat_id` is unused/meaningless (never itself handed to a
    /// tool). See `ToolContext`'s own doc comment for why it isn't `AppState`.
    tool_context: ToolContext,
    /// Where finished background jobs are looked up when a chat's next turn starts — see
    /// `flush_job_notices`. Also reachable to tools through `tool_context`.
    job_store: Arc<JobStore>,
    /// Per-chat tool-permission grants — what scope each tool has already been given
    /// within a given chat, if any. Consulted by `to_agent_tool_call`/`use_tool` to
    /// decide whether a call is `Allowed` outright or needs the caller to confirm.
    permission_store: Arc<PermissionStore>,
    /// Per-user settings — consulted once per turn for the user's custom system prompt
    /// (a user without one gets the built-in default instead).
    settings_store: Arc<SettingsStore>,
    /// See `INLINED_RESULT_FRACTION` — the most of a sub-agent's result a notice carries.
    max_inlined_result_bytes: u64,
    /// Chats with a tool call executing right now. See `RunningToolGuard`.
    running_tools: Arc<Mutex<HashSet<i64>>>,
    /// Sub-agent chats whose run is going on right now (from being started until it ends or is
    /// killed). Their tool calls are the backend's to run, so `is_running` treats them as busy —
    /// without it, opening one between two of its calls would look like a call cut short by a restart.
    live_subagents: Arc<Mutex<HashSet<i64>>>,
    /// One permit: sub-agents run one at a time. They all use the same model on the same GPU, and
    /// two of them taking turns would each evict the other's cached prompt on every call — slower
    /// for both than running back to back.
    subagent_slot: Arc<Semaphore>,
}

/// Marks a chat as having a tool call executing, for as long as it lives. Dropping it — on
/// completion, on an error, or because the request was cancelled (a client that disconnects drops
/// the handler's future) — clears the mark. Without it a chat with an *allowed* call in flight is
/// indistinguishable from one whose call was cut short by a restart: both look like "allowed and
/// still unresolved". The same guard marks a sub-agent's chat for its whole run
/// (`live_subagents`), for the same reason.
struct RunningToolGuard {
    running: Arc<Mutex<HashSet<i64>>>,
    chat_id: i64,
}

impl Drop for RunningToolGuard {
    fn drop(&mut self) {
        self.running.lock().unwrap().remove(&self.chat_id);
    }
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
        Self {
            history,
            model,
            compaction,
            chat_store,
            tools,
            tool_context,
            job_store,
            permission_store,
            settings_store,
            max_inlined_result_bytes: (context_length as f64 * INLINED_RESULT_FRACTION * INLINED_RESULT_CHARS_PER_TOKEN)
                as u64,
            running_tools: Arc::new(Mutex::new(HashSet::new())),
            live_subagents: Arc::new(Mutex::new(HashSet::new())),
            subagent_slot: Arc::new(Semaphore::new(1)),
        }
    }

    /// Claims the chat's single tool-execution slot. Two calls running at once for one chat (a
    /// second tab opened, or a reload, while a long command is still going) would run the same
    /// pending call twice.
    fn mark_running(&self, chat_id: i64) -> Result<RunningToolGuard, ErrorService> {
        if !self.running_tools.lock().unwrap().insert(chat_id) {
            return Err(ErrorService::new(StatusCode::CONFLICT, "a tool call is already running for this chat"));
        }
        Ok(RunningToolGuard { running: self.running_tools.clone(), chat_id })
    }

    fn is_running(&self, chat_id: i64) -> bool {
        self.running_tools.lock().unwrap().contains(&chat_id) || self.live_subagents.lock().unwrap().contains(&chat_id)
    }

    /// Records the tool calls that were cut short as interrupted, instead of leaving them to be
    /// run again. Opening a chat is where this is discovered: a call the chat's grants already
    /// allow is executed the moment the model asks for it, never left waiting for a person — so
    /// one that is *still* unresolved while nothing is executing was interrupted (the backend
    /// restarted, or the client went away mid-run), and re-running it could repeat something that
    /// already happened, or block again on a command that never exits. A call waiting for the
    /// user's confirmation is different and stays as it is; so does everything queued after it,
    /// since results are recorded in the order the model asked.
    async fn settle_interrupted(&self, chat_id: i64) -> Result<(), ErrorService> {
        if self.is_running(chat_id) {
            return Ok(());
        }

        for call in self.pending_tool_calls(chat_id).await? {
            let name = call.tool_name.clone();
            let view = self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?;
            if !matches!(view.permission, AgentToolPermission::Allowed) {
                break;
            }

            self.chat_store
                .new_message(NewMessage {
                    chat_id,
                    role: "tool".to_string(),
                    content: Value::String(INTERRUPTED_TOOL_MESSAGE.to_string()).to_string(),
                    tool_name: Some(name),
                    thinking: None,
                    thought_duration_ms: None,
                    tool_success: Some(false),
                    tool_denied: false,
                    tool_calls: vec![],
                    images: vec![],
                    file_ids: vec![],
                    prompt_tokens: None,
                    eval_tokens: None,
                    timings: MessageTimings::default(),
                })
                .await?;
        }
        Ok(())
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

        let notices = self.flush_job_notices(chat_id).await?;

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
        let notices = self.flush_job_notices(chat_id).await?;
        let messages = self.history.for_chat(chat_id).await?;
        self.advance(chat_id, messages, None, think, notices, true).await
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

    /// Persists a `notice` for every background job (or sub-agent) that has finished since the model
    /// was last told about one, and returns them — without calling the model. A client shows them at
    /// once and then has the model respond with `continue_chat`; splitting it that way is what lets
    /// the notice appear the moment the job ends instead of after a model call that can take a
    /// while. Empty if there's nothing to report, which is what makes a stale hint
    /// (`ServerEvent::JobFinished` for a job an in-progress turn already reported) harmless — the
    /// decision is made here, not by the caller, because only here is claiming the finished jobs
    /// atomic. Also empty while tool calls are still waiting to run (mid-turn, or paused on a
    /// confirmation): a notice has to come after every tool result already in the chat, never in the
    /// middle of an unfinished batch, so it goes out with the `continue_chat` that follows once
    /// they have.
    pub async fn flush_notices(&self, chat_id: i64) -> Result<Vec<NoticeOut>, ErrorService> {
        // Possible: also return empty while a model call is in flight (see TOOLS.md, "Known gap").
        if !self.pending_tool_calls(chat_id).await?.is_empty() {
            return Ok(vec![]);
        }

        self.flush_job_notices(chat_id).await
    }

    /// Turns every background job of `chat_id` that has finished but not been reported
    /// into a persisted `notice` message, oldest first, and returns them. Called at the
    /// one point in every turn where appending is always safe — after the newest
    /// message already stored, before the model's own reply — so a notice can never
    /// land between an assistant message's tool calls and their results. Persisted (not
    /// just added to the prompt) so the model keeps seeing it on later turns, the chat
    /// reads the same after a reload as it did live, and the reply that follows makes
    /// sense next to it. Each job is claimed before its notice is written, so
    /// concurrent callers can't report it twice; a job whose notice fails to save is
    /// handed back so the next turn tries again.
    async fn flush_job_notices(&self, chat_id: i64) -> Result<Vec<NoticeOut>, ErrorService> {
        let jobs = self.job_store.claim_unnotified(chat_id).await?;

        let mut notices = Vec::with_capacity(jobs.len());
        for job in jobs {
            let content = match job.kind {
                JobKind::Process => job_notice_text(&job),
                JobKind::Agent { .. } => self.agent_job_notice_text(&job).await,
            };
            let stored = self
                .chat_store
                .new_message(NewMessage {
                    chat_id,
                    role: "notice".to_string(),
                    content,
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
                .await;

            match stored {
                Ok(message) => notices.push(NoticeOut { content: message.content, created_at: message.created_at }),
                Err(e) => {
                    if let Err(undo) = self.job_store.unclaim(job.id).await {
                        tracing::error!(job_id = job.id, "couldn't hand a job back after its notice failed to save: {undo}");
                    }
                    return Err(e.into());
                }
            }
        }

        Ok(notices)
    }

    /// The `notice` text for a finished sub-agent job. Unlike a command's, it carries the outcome
    /// itself: the result of a run that succeeded, or why it didn't get one.
    async fn agent_job_notice_text(&self, job: &JobRecord) -> String {
        let prompt = command_preview(&job.command);

        match (job.status, job.exit_code) {
            (JobStatus::Lost, _) => prompts::subagent_job_notice(job.id, &prompt, SubagentEnd::Lost),
            (JobStatus::Exited, code) => {
                let (text, cut) = match self.job_store.read_log_head(job, self.max_inlined_result_bytes).await {
                    Ok(read) => read,
                    Err(e) => (format!("(its result could not be read: {e})"), false),
                };
                let end = if code == Some(0) {
                    SubagentEnd::Finished { result: &text, cut }
                } else {
                    SubagentEnd::Failed { result: &text, cut }
                };
                prompts::subagent_job_notice(job.id, &prompt, end)
            }
            _ => prompts::subagent_job_notice(job.id, &prompt, SubagentEnd::Ended),
        }
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
                self.to_agent_tool_call(chat_id, call.function.name, call.function.arguments)
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

    /// The tool calls the model has asked for that haven't been run yet, without
    /// actually running them — lets a caller check each one's `permission` (and warn
    /// about a `Denied` one) before committing to `use_tool`.
    ///
    /// Also settles calls that were cut short (see `settle_interrupted`): they're recorded as
    /// interrupted rather than reported as pending, so opening a chat never re-runs them. While a
    /// call is executing, nothing is reported as pending — whoever is running it owns it.
    pub async fn can_use_tool(&self, chat_id: i64) -> Result<CanUseTool, ErrorService> {
        if self.is_running(chat_id) {
            return Ok(CanUseTool { can_use: false, tools: vec![] });
        }
        self.settle_interrupted(chat_id).await?;

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
        self.run_next_tool(chat_id, scope, false).await
    }

    /// `use_tool`'s body. `unattended` is for a chat nobody is watching (a sub-agent's): a denied
    /// call is reported to the model as final for this run, instead of promising it "they'll be
    /// asked to approve it then" — there is no one to ask.
    async fn run_next_tool(&self, chat_id: i64, scope: Option<Value>, unattended: bool) -> Result<UseToolOut, ErrorService> {
        let _running = self.mark_running(chat_id)?;
        let chat = self.chat_store.chat(chat_id).await?;
        let is_subagent = chat.parent_chat_id.is_some();
        let mut pending = self.pending_tool_calls(chat_id).await?.into_iter();
        let next = pending
            .next()
            .ok_or_else(|| ErrorService::new(StatusCode::BAD_REQUEST, "no pending tool call to run"))?;

        let had_scope;
        let effective_scope = match scope {
            Some(scope) => {
                had_scope = true;
                match self.tools.get_tool(&next.tool_name).await {
                    Some(tool) => resolved_scope_from_json(tool.as_ref(), scope),
                    None => ResolvedScope::default(),
                }
            }
            None => {
                let stored = self.stored_scope(chat_id, &next.tool_name).await?;
                had_scope = stored.own.is_some() || !stored.shared.is_empty();
                stored
            }
        };

        let permission = self
            .tool_permission(&next.tool_name, next.arguments.clone(), effective_scope, is_subagent)
            .await;

        let (success, denied, err, content) = match permission {
            AgentToolPermission::Allowed => {
                let ctx = self.tool_context.copy_with_chat_id(chat_id, chat.user_id, chat.provider, chat.model);
                match self.tools.call_tool(&next.tool_name, next.arguments, &ctx).await {
                    Ok(value) => (true, false, None, value),
                    Err(e) => {
                        let message = e.to_string();
                        (false, false, Some(message.clone()), Value::String(message))
                    }
                }
            }
            AgentToolPermission::Denied { reason, escalation } => {
                let message = prompts::tool_call_refused(unattended, had_scope, escalation.is_some(), &next.tool_name, &reason);
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
                images: vec![],
                file_ids: vec![],
                prompt_tokens: None,
                eval_tokens: None,
                timings: MessageTimings::default(),
            })
            .await?;

        let mut tools = Vec::with_capacity(pending.len());
        for call in pending {
            tools.push(self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?);
        }

        Ok(UseToolOut {
            id: stored.id,
            success,
            denied,
            tool_name: next.tool_name,
            err,
            content,
            created_at: stored.created_at,
            tools,
        })
    }

    /// Persists a scope grant for a tool within a chat, so future calls to that tool (or
    /// any other tool sharing one of its buckets — see `Tool::shared_buckets`) can be
    /// `Allowed` without asking again. `scope` is the envelope `resolved_scope_to_json`
    /// produced when this grant was first offered, echoed back verbatim by the
    /// frontend; `resolved_scope_from_json` reads it back into its own/shared-bucket
    /// deltas — each one is just the single new fact that call needed (see
    /// `storage::check_scope`), not a snapshot of everything already granted. Each delta
    /// is appended to that row's *current* value, read fresh right here rather than
    /// trusted from whatever the caller last saw: two denied calls from the same reply
    /// needing the same bucket have their escalations computed from the same
    /// pre-approval state, so if this just overwrote with the caller's delta, approving
    /// the second would erase the first's grant. Reading fresh at the moment each one is
    /// actually persisted is what makes approving both, in sequence, correct.
    pub async fn allow_scope(&self, chat_id: i64, tool_name: String, scope: Value) -> Result<(), ErrorService> {
        let Some(tool) = self.tools.get_tool(&tool_name).await else {
            return Err(ErrorService::new(
                StatusCode::BAD_REQUEST,
                format!("no tool named '{tool_name}'"),
            ));
        };

        let delta = resolved_scope_from_json(tool.as_ref(), scope);

        if let Some(own_delta) = delta.own {
            let existing = self.get_scope_or_none(chat_id, &tool_name).await?;
            self.permission_store.update_scope(chat_id, &tool_name, merge_scope_delta(existing, own_delta)).await?;
        }
        for (bucket, shared_delta) in delta.shared {
            let existing = self.get_scope_or_none(chat_id, bucket.db_key()).await?;
            self.permission_store
                .update_scope(chat_id, bucket.db_key(), merge_scope_delta(existing, shared_delta))
                .await?;
        }

        Ok(())
    }

    /// A tool's actual scope for one call — its own bucket (if it has one) plus every
    /// shared bucket it declares, each fetched and kept separate rather than flattened
    /// into one object. Flattening would collide: every storage bucket stores its grant
    /// under the same JSON key (`SharedBucket::json_key`), so a tool declaring both
    /// `StorageRead` and `StorageWrite` would have one silently overwrite the other if
    /// they were merged into a single map instead of kept apart by bucket.
    async fn stored_scope(&self, chat_id: i64, tool_name: &str) -> Result<ResolvedScope, ErrorService> {
        let Some(tool) = self.tools.get_tool(tool_name).await else {
            return Ok(ResolvedScope::default());
        };

        let own = if tool.uses_own_bucket() {
            self.get_scope_or_none(chat_id, tool_name).await?
        } else {
            None
        };

        let mut shared = HashMap::new();
        for &bucket in tool.shared_buckets() {
            if let Some(value) = self.get_scope_or_none(chat_id, bucket.db_key()).await? {
                shared.insert(bucket, value);
            }
        }

        Ok(ResolvedScope { own, shared })
    }

    /// `PermissionStore::get_scope`, with "nothing granted yet" collapsed to `None`
    /// rather than an error — that's the normal/expected case for most tool calls, not
    /// a failure.
    async fn get_scope_or_none(&self, chat_id: i64, key: &str) -> Result<Option<Value>, ErrorService> {
        match self.permission_store.get_scope(chat_id, key).await {
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
    async fn tool_permission(
        &self,
        tool_name: &str,
        data: Value,
        scope: ResolvedScope,
        is_subagent: bool,
    ) -> AgentToolPermission {
        if !subagent::available_to(tool_name, is_subagent) {
            return AgentToolPermission::Denied {
                reason: format!("'{tool_name}' isn't available in this chat"),
                escalation: None,
            };
        }

        let Some(tool) = self.tools.get_tool(tool_name).await else {
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
                    scope: resolved_scope_to_json(grant.scope),
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
        let (messages, _) = self.chat_store.messages(chat_id, PENDING_TOOL_CALLS_LOOKBACK, 0).await?;

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
        let is_subagent = self.chat_store.chat(chat_id).await?.parent_chat_id.is_some();
        let permission = self.tool_permission(&name, arguments.clone(), scope, is_subagent).await;

        Ok(AgentToolCall {
            permission,
            name,
            arguments,
        })
    }
}

/// Serializes a `ResolvedScope` into the flat JSON value that crosses the HTTP boundary
/// as `AgentScopeGrant.scope` — opaque to the frontend (`unknown` on its side), which
/// only ever echoes it back verbatim via `allow_scope` or a `use_tool` one-time
/// override. Shared buckets are keyed by `SharedBucket::db_key()` — the same string
/// `PermissionStore` rows already use — so `resolved_scope_from_json` (below) can read
/// them back into the right bucket unambiguously, rather than guessing a flattened
/// object apart by a shared JSON key the way the old `split_scope` had to.
fn resolved_scope_to_json(scope: ResolvedScope) -> Value {
    let shared: serde_json::Map<String, Value> =
        scope.shared.into_iter().map(|(bucket, value)| (bucket.db_key().to_string(), value)).collect();

    serde_json::json!({ "own": scope.own, "shared": shared })
}

/// The inverse of `resolved_scope_to_json`. `tool` decides which shared buckets are even
/// meaningful for it; a bucket key present in `json` that `tool` doesn't declare (e.g.
/// stale data from before a tool's declared buckets changed) is just dropped rather than
/// erroring — there's nothing sensible to do with it, and dropping it is no worse than
/// the grant never having existed.
fn resolved_scope_from_json(tool: &dyn Tool, json: Value) -> ResolvedScope {
    let own = json.get("own").filter(|v| !v.is_null()).cloned();

    let mut shared = HashMap::new();
    if let Some(shared_obj) = json.get("shared").and_then(|s| s.as_object()) {
        for &bucket in tool.shared_buckets() {
            if let Some(value) = shared_obj.get(bucket.db_key()) {
                shared.insert(bucket, value.clone());
            }
        }
    }

    ResolvedScope { own, shared }
}

/// Appends `delta`'s facts into `existing`, one level deep: for a top-level key that's
/// an object on both sides (e.g. `"folders"`), the two objects' own keys are unioned —
/// two different approved folders both end up in the same map, rather than the second
/// replacing the first. Anything else in `delta` just sets that key outright. Every
/// bucket's stored shape today is exactly one level deep (`{"folders": {...}}`,
/// `{"hosts": {...}}`), so one level of recursion covers everything currently in play.
fn merge_scope_delta(existing: Option<Value>, delta: Value) -> Value {
    let mut base = existing.and_then(|v| v.as_object().cloned()).unwrap_or_default();
    let Some(delta_obj) = delta.as_object() else {
        return delta;
    };

    for (key, delta_value) in delta_obj {
        match (base.get(key).and_then(|v| v.as_object()), delta_value.as_object()) {
            (Some(existing_inner), Some(delta_inner)) => {
                let mut merged_inner = existing_inner.clone();
                merged_inner.extend(delta_inner.clone());
                base.insert(key.clone(), Value::Object(merged_inner));
            }
            _ => {
                base.insert(key.clone(), delta_value.clone());
            }
        }
    }

    Value::Object(base)
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

