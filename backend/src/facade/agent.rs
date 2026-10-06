use std::sync::Arc;

use sea_orm::prelude::DateTimeUtc;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, Semaphore};
use utoipa::ToSchema;

use crate::services::{
    chat_store::ChatStore,
    error::ErrorService,
    event_bus::{EventBus, ServerEvent},
    file_store::FileStore,
    job_store::JobStore,
    llm::{LlmProviders, ThinkChoice},
    permission_store::PermissionStore,
    preset_store::PresetStore,
    settings_store::SettingsStore,
    tools::ToolService,
};
use crate::facade::launch::LaunchFacade;
use crate::facade::one_shot::OneShot;
use crate::tools::base::ToolContext;
use crate::tools::subagent::SubagentHandle;

mod compaction;
mod history;
mod model_call;
mod notices;
mod prompts;
mod run_tracker;
mod runner;
mod subagent_run;
mod tool_calls;
mod turn;

use compaction::Compaction;
use history::History;
use notices::Notices;
use tool_calls::ToolCalls;
use turn::Turn;
pub use runner::{Allowance, Decision, RunEnded, StartedTurn, TurnState, TurnStatus};
use runner::TurnRunner;
use model_call::ModelCall;
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
    /// Provider, launch profile, call parameters and the turn's claim on the model server, and what
    /// is done with a reply that can't be used.
    model: ModelCall,
    /// The loop of a turn, one run per chat.
    runner: TurnRunner,
    chat_store: Arc<ChatStore>,
    /// The services the agent itself reaches (files, events), and the template `ToolCalls` makes each
    /// tool's real, per-call context from. See `ToolContext`'s own doc comment for why it isn't `AppState`.
    tool_context: ToolContext,
    /// The model's tool calls: which are pending, whether each is permitted, running them.
    tool_calls: ToolCalls,
    /// Per-user settings: auto-confirm for a sub-agent's run.
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
        let tool_calls = ToolCalls::new(chat_store.clone(), tools.clone(), permission_store, tool_context.clone(), model.clone());
        let turn = Turn::new(
            chat_store.clone(),
            tools.clone(),
            tool_context.events.clone(),
            history.clone(),
            model.clone(),
            compaction.clone(),
            tool_calls.clone(),
        );
        let notices = Notices::new(
            chat_store.clone(),
            job_store,
            (context_length as f64 * INLINED_RESULT_FRACTION * INLINED_RESULT_CHARS_PER_TOKEN) as u64,
        );
        let runner = TurnRunner::new(
            chat_store.clone(),
            settings_store.clone(),
            tool_context.file_store.clone(),
            tool_context.events.clone(),
            history,
            notices,
            tool_calls.clone(),
            turn,
        );
        Self {
            model,
            runner,
            tool_calls,
            chat_store,
            tool_context,
            settings_store,
            subagent_slot: Arc::new(Semaphore::new(1)),
        }
    }

    /// Stores `prompt` (plus `images`, if any — base64-encoded, no data-URL prefix — and `file_ids`, if
    /// any) as a `user` message and starts a run that answers it, returning at once: the loop of model
    /// calls and tool calls goes on in the background (see `TurnRunner`). The prompt is saved before any
    /// model call, so a failed or slow one never loses what the user sent. `think` is forwarded to the
    /// model provider as-is. Refused with 409 while the chat has a run going on.
    pub async fn start_turn(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        file_ids: Vec<i64>,
        think: Option<ThinkChoice>,
    ) -> Result<StartedTurn, ErrorService> {
        self.runner.start_prompt(chat_id, prompt, images, file_ids, think).await
    }

    /// Has the model answer again where it answered last, in the background, and replaces that answer
    /// with the new one — see `TurnRunner::start_regenerate` for what qualifies.
    pub async fn start_regenerate(&self, chat_id: i64, message_id: i64, think: Option<ThinkChoice>) -> Result<(), ErrorService> {
        self.runner.start_regenerate(chat_id, message_id, think).await
    }

    /// Answers the permission prompt a chat is waiting at and continues its turn in the background.
    pub async fn answer(&self, chat_id: i64, decisions: Vec<Decision>, think: Option<ThinkChoice>) -> Result<(), ErrorService> {
        self.runner.start_answer(chat_id, decisions, think).await
    }

    /// Stops the chat's run: the model call in flight is dropped. A tool already running finishes.
    pub fn stop(&self, chat_id: i64) -> Result<(), ErrorService> {
        self.runner.stop(chat_id)
    }

    /// Whether the chat has a run going on.
    pub fn has_run(&self, chat_id: i64) -> bool {
        self.runner.has_run(chat_id)
    }

    /// What the chat's turn is doing: nothing, running (since when, how many tokens so far), or
    /// waiting at a permission prompt (which calls).
    pub async fn turn_state(&self, chat_id: i64) -> Result<TurnState, ErrorService> {
        self.runner.state(chat_id).await
    }

    /// Stores `prompt` and answers it with one model call and no tools: a messaging plugin's agent,
    /// whose reply is sent back to the messaging app by its caller.
    pub async fn reply(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        think: Option<ThinkChoice>,
    ) -> Result<ChatOut, ErrorService> {
        self.runner.reply_once(chat_id, prompt, images, think).await
    }

    /// Makes a finished background job (or sub-agent) start a run on its chat by itself, with no browser
    /// open: its notice reaches the model and the model answers. Called once, for the agent whose chats
    /// are the users' own (the messaging plugins' agent has no tools and so no jobs).
    pub fn bind_job_waker(self: &Arc<Self>) {
        let runner = self.runner.clone();
        let mut events = self.tool_context.events.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(ServerEvent::JobFinished { chat_id, .. }) => {
                        if let Err(e) = runner.wake(chat_id).await {
                            tracing::warn!(chat_id, "couldn't start a run for a finished job: {}", e.message.as_deref().unwrap_or("unknown error"));
                        }
                    }
                    // Events are hints (see `ServerEvent`): one that was missed only delays a notice until the
                    // chat's next turn
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
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
    /// set (unlike the same-named field on `Message`/`NewMessage`) — a step times
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

