use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{Stream, StreamExt};
use tokio::sync::broadcast::Receiver;
use tokio_stream::wrappers::BroadcastStream;

use crate::{
    routes::auth::AuthUser,
    services::{chat_store::ChatStore, error::ErrorService, event_bus::ServerEvent},
    state::AppState,
};

/// A long-lived Server-Sent Events stream of everything the backend wants to tell the signed-in
/// user's frontend about outside any request it was asked to answer — today just
/// `job_finished` (`{"type": "job_finished", "chat_id": .., "job_id": ..}`) when a
/// background job ends. Every event is an unnamed SSE message whose JSON `data` carries
/// its `type`, so a client needs a single listener however many event types exist.
/// One-way, server to client: a frontend
/// reacts by making the normal API calls it always makes. Each event is only a hint
/// that something changed — see `ServerEvent` — so a client that misses one (a dropped
/// connection, a tab opened later) loses nothing but promptness. Nothing is replayed to
/// a client that connects late.
///
/// The event bus is shared by every user, so each subscriber only receives the events about
/// chats it owns (one ownership lookup per event, and events are rare).
#[utoipa::path(
    get,
    path = "/api/events",
    tag = "events",
    responses(
        (status = 200, description = "An open `text/event-stream`; each event's data is a JSON `ServerEvent`", content_type = "text/event-stream", body = String),
        (status = 401, description = "Not signed in", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn stream(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ErrorService> {
    let chats = state.services().await?.chat_store;

    let events = events_for(state.events.subscribe(), chats, auth.id)
        .filter_map(|event| async move { Some(Ok(Event::default().json_data(&event).ok()?)) });

    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

/// The bus's events that concern chats `user_id` owns. A subscriber that falls too far behind gets
/// a `Lagged` error in place of the events it missed — skipped here, since each event is only a
/// hint (see `stream`).
fn events_for(
    events: Receiver<ServerEvent>,
    chats: Arc<ChatStore>,
    user_id: i64,
) -> impl Stream<Item = ServerEvent> {
    BroadcastStream::new(events).filter_map(move |received| {
        let chats = chats.clone();
        async move {
            let event = received.ok()?;
            chats.owned_chat(user_id, event.chat_id()).await.ok()?;
            Some(event)
        }
    })
}
