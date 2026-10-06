use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{facade::agent::TurnState, routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub struct TurnStateQuery {
    pub chat_id: i64,
}

/// What `chat_id`'s turn is doing: `idle`, `running` (since when, when the current model call started and the
/// tokens its finished calls generated, so a reloaded page can show "thinking for 1m 30s, N tokens" from the
/// real start), or `waiting_for_permission` (with the tool calls waiting for an answer, which `POST
/// /api/agent/answer` takes by position). Also says how the last run ended. On a chat with no run, a tool call
/// the backend was cut off in the middle of (a restart) is recorded as interrupted here.
#[utoipa::path(
    get,
    path = "/api/agent/turn",
    tag = "agent",
    params(TurnStateQuery),
    responses(
        (status = 200, description = "The chat's turn state", body = TurnState),
        (status = 404, description = "Chat not found", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn turn_state(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<TurnStateQuery>,
) -> Result<Json<TurnState>, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, query.chat_id).await?;
    Ok(Json(services.agent.turn_state(query.chat_id).await?))
}
