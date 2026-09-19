use serde::Serialize;
use tokio::sync::broadcast;
use utoipa::ToSchema;

/// How many undelivered events a slow subscriber may fall behind by before it starts
/// missing the oldest ones. Events are tiny and rare (one per finished background job),
/// so this only ever matters for a connection stalled for a long time.
const CHANNEL_CAPACITY: usize = 64;

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
