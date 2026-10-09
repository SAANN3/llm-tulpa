use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct UnseenChatsResponse {
    /// The caller's chats (those the chat list shows) whose last run ended in something not looked at yet
    chat_ids: Vec<i64>,
}

/// The caller's chats with news: their last run answered, failed, stopped at the step limit or is waiting for
/// permission, and the chat hasn't been opened since. A page keeps the set current from the events (`run_ended`,
/// `chat_seen`, `run_started`, `chat_deleted`) and reads it again when the event stream reopens.
#[utoipa::path(
    get,
    path = "/api/chats/unseen",
    tag = "chats",
    responses((status = 200, description = "The chats with news", body = UnseenChatsResponse)),
)]
pub async fn unseen_chats(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<UnseenChatsResponse>, ErrorService> {
    let services = state.services().await?;
    Ok(Json(UnseenChatsResponse { chat_ids: services.chat_store.unseen_chat_ids(auth.id).await? }))
}
