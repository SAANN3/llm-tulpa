use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

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
    path = "/api/live",
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
        .take_until(state.shutdown.clone().cancelled_owned())
        .filter_map(|event| async move { Some(Ok(Event::default().json_data(&event).ok()?)) });

    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}

/// The bus's events that concern chats `user_id` owns. A subscriber that falls too far behind gets
/// a `Lagged` error in place of the events it missed — skipped here, since each event is only a
/// hint (see `stream`). Whose a chat is is asked once per chat for the stream's whole life: a chat
/// never changes owner, and a reply being written sends several events a second about the same chat,
/// which would otherwise be a query each, and a stream slowed by them falls behind and loses events.
fn events_for(
    events: Receiver<ServerEvent>,
    chats: Arc<ChatStore>,
    user_id: i64,
) -> impl Stream<Item = ServerEvent> {
    let owned: Arc<Mutex<HashMap<i64, bool>>> = Arc::default();
    BroadcastStream::new(events).filter_map(move |received| {
        let chats = chats.clone();
        let owned = owned.clone();
        async move {
            let event = received.ok()?;
            // An event about no chat in particular (the model server's state) is everyone's
            if let Some(chat_id) = event.chat_id() {
                let known = owned.lock().unwrap().get(&chat_id).copied();
                let mine = match known {
                    Some(mine) => mine,
                    // A failed lookup drops this event and is not remembered, so the next one asks again
                    None => {
                        let mine = chats.owns(user_id, chat_id).await.ok()?;
                        owned.lock().unwrap().insert(chat_id, mine);
                        mine
                    }
                };
                mine.then_some(())?;
            }
            Some(event)
        }
    })
}
