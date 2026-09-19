use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};

use crate::state::AppState;

/// A long-lived Server-Sent Events stream of everything the backend wants to tell a
/// connected frontend about outside any request it was asked to answer — today just
/// `job_finished` (`{"type": "job_finished", "chat_id": .., "job_id": ..}`) when a
/// background job ends. Every event is an unnamed SSE message whose JSON `data` carries
/// its `type`, so a client needs a single listener however many event types exist.
/// One-way, server to client: a frontend
/// reacts by making the normal API calls it always makes. Each event is only a hint
/// that something changed — see `ServerEvent` — so a client that misses one (a dropped
/// connection, a tab opened later) loses nothing but promptness. Nothing is replayed to
/// a client that connects late.
#[utoipa::path(
    get,
    path = "/api/events",
    tag = "events",
    responses(
        (status = 200, description = "An open `text/event-stream`; each event's data is a JSON `ServerEvent`", content_type = "text/event-stream", body = String),
    ),
)]
pub async fn stream(State(state): State<Arc<AppState>>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // A subscriber that falls too far behind gets a `Lagged` error in place of the
    // events it missed — skipped here, since each event is only a hint (see above).
    let events = BroadcastStream::new(state.events.subscribe()).filter_map(|received| {
        let event = received.ok()?;
        let sse = Event::default().json_data(&event).ok()?;
        Some(Ok(sse))
    });

    Sse::new(events).keep_alive(KeepAlive::default())
}
