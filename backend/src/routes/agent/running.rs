use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    facade::agent::{TurnState, TurnStatus},
    routes::auth::AuthUser,
    services::error::ErrorService,
    state::AppState,
};

#[derive(Serialize, ToSchema)]
pub(crate) struct RunningChat {
    chat_id: i64,
    /// The chat that started this one as a sub-agent, or `null`: a page with no row for a sub-agent's chat
    /// shows its work on this one
    parent_chat_id: Option<i64>,
    /// The same record `GET /api/agent/turn` gives for the chat
    state: TurnState,
}

/// The caller's chats that have a run going on right now (a sub-agent's chat included), each with its turn
/// state. A chat waiting for a permission answer has no run and is not listed: `GET /api/agent/turn` says so.
#[utoipa::path(
    get,
    path = "/api/agent/runs",
    tag = "agent",
    responses(
        (status = 200, description = "The caller's running chats", body = Vec<RunningChat>),
    ),
)]
pub async fn running(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<Vec<RunningChat>>, ErrorService> {
    let services = state.services().await?;
    let running = services.agent.running_chat_ids();
    let mut chats = Vec::new();
    for chat in services.chat_store.owned_chat_refs(auth.id, &running).await? {
        let turn = services.agent.turn_state(chat.id).await?;
        // A run that ended between the two reads is not running any more
        if matches!(turn.status, TurnStatus::Running) {
            chats.push(RunningChat { chat_id: chat.id, parent_chat_id: chat.parent_chat_id, state: turn });
        }
    }
    Ok(Json(chats))
}
