use sea_orm::prelude::DateTimeUtc;
use serde::Serialize;
use tokio::sync::broadcast;
use utoipa::ToSchema;

/// How many undelivered events a slow subscriber may fall behind by before it starts
/// missing the oldest ones. Events are tiny and come a few at a time (a run's messages and tool
/// calls, a finished background job), so this only ever matters for a connection stalled for a long time.
const CHANNEL_CAPACITY: usize = 256;

/// Something the backend tells every connected frontend about, outside any request it
/// was asked to answer — see `GET /api/events`. Deliberately just a hint that something
/// changed (which chat, which job), never the content itself: the actual data is always
/// read back through the normal API, so a missed or duplicated event can't leave a
/// client with wrong data, only a late refresh.
///
/// Adding an event is adding a variant here and nothing else on the backend: every
/// event goes out on the same stream as an unnamed SSE message whose JSON carries its
/// `type`, and anything that wants to publish one already has the bus — routes through
/// `AppState::events`, tools through `ToolContext::events`, a service through the
/// `Arc<EventBus>` handed to its constructor (see `JobStore`). The frontend side is one
/// line in the `ServerEvent` union in `hooks/useServerEvents.ts`.
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    /// A background job stopped on its own (exited, or was lost) and hasn't been
    /// reported to its chat's model yet — see `JobStore::claim_unnotified`.
    JobFinished { chat_id: i64, job_id: i64 },
    /// A model call in `chat_id`'s turn just finished, generating `eval_tokens` tokens
    /// (its Ollama `eval_count`) at a running context usage of `prompt_tokens` (its
    /// Ollama `prompt_eval_count`, `None` when Ollama didn't report one) — a spend hint
    /// for the live "thinking" indicator and, via `prompt_tokens`, the only way the
    /// context-usage bar in the header can move *during* a multi-tool-call turn: a
    /// turn's own `ChatOut` (with the same two numbers) only reaches the frontend once
    /// the whole turn is done, but several Ollama calls can happen inside one turn (a
    /// tool call, then another, then the final reply), each with its own numbers. The
    /// counts arrive in model-call granularity, not per token: Ollama is called
    /// non-streaming, so each jumps by a chunk each time a call returns. Clients sum the
    /// `eval_tokens` deltas for the duration of one turn but take `prompt_tokens` as-is
    /// (it's already cumulative, not a delta).
    TurnProgress { chat_id: i64, eval_tokens: u64, prompt_tokens: Option<u64>, step: u32 },
    /// A run (the backend's loop of model calls and tool calls for one chat) started.
    RunStarted {
        chat_id: i64,
        #[schema(value_type = String, format = "date-time")]
        started_at: DateTimeUtc,
    },
    /// Messages were stored in the chat (the user's, a reply, a tool result, a notice): read the ones
    /// newer than the last one the client has.
    MessagesChanged { chat_id: i64 },
    /// A tool call is about to run.
    ToolStarted { chat_id: i64, tool_name: String },
    /// The run on the chat ended, and why. For `waiting_for_permission`, `GET /api/agent/turn` says what is asked.
    RunEnded {
        chat_id: i64,
        reason: RunEndReason,
        detail: Option<String>,
        /// When the run started, and the tokens its model calls generated: "stopped after 2m 10s, 3.4k tokens".
        #[schema(value_type = String, format = "date-time")]
        started_at: DateTimeUtc,
        eval_tokens: u64,
        /// The HTTP status the failure would have had (423 when the model server is in use by someone else).
        status: Option<u16>,
    },
    /// The model server started loading, became ready, stopped or failed. Not about any one chat:
    /// everyone is told, so a page can say "the model is being applied, don't close it".
    ModelState {
        state: ModelStateKind,
        /// The launch profile that is loading or loaded
        profile_id: Option<i64>,
        /// The model's file name
        model: Option<String>,
        /// Why it failed or stopped, when there is something to say
        detail: Option<String>,
    },
}

/// Why a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunEndReason {
    /// The model replied without asking for a tool.
    Answered,
    /// A model call or a store failed; `detail` says what.
    Failed,
    /// The user stopped it.
    Stopped,
    /// The user's step limit was reached and the model was asked to conclude.
    StepLimit,
    /// A tool call needs the user's permission; the run continues with their answer.
    WaitingForPermission,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelStateKind {
    Loading,
    Ready,
    Stopped,
    Failed,
    /// Somebody's request is waiting for the model to be free of the turn that has it
    Queued,
}

impl ServerEvent {
    /// The chat this event is about, or `None` for one that concerns everybody. The bus is shared by
    /// every connected user, so the event stream (`routes/events/stream.rs`) delivers a chat's event
    /// only to the owner of that chat — a new variant has to say which chat (and so which user) it
    /// concerns, or be deliberately global, or the stream has no way to keep it from everyone else.
    pub fn chat_id(&self) -> Option<i64> {
        match self {
            ServerEvent::JobFinished { chat_id, .. } => Some(*chat_id),
            ServerEvent::TurnProgress { chat_id, .. } => Some(*chat_id),
            ServerEvent::RunStarted { chat_id, .. } => Some(*chat_id),
            ServerEvent::MessagesChanged { chat_id } => Some(*chat_id),
            ServerEvent::ToolStarted { chat_id, .. } => Some(*chat_id),
            ServerEvent::RunEnded { chat_id, .. } => Some(*chat_id),
            ServerEvent::ModelState { .. } => None,
        }
    }
}

/// One-to-many fan-out of `ServerEvent`s to whoever is connected right now. Nothing is
/// buffered for a client that isn't connected — an event with no subscribers is simply
/// dropped, which is fine because every event is only a hint (see `ServerEvent`) and
/// whatever it points at is still there to be found the next time that client asks.
pub struct EventBus {
    sender: broadcast::Sender<ServerEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    pub fn publish(&self, event: ServerEvent) {
        // `Err` only means nobody is subscribed right now — see the type's doc comment.
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.sender.subscribe()
    }
}
