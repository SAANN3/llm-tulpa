use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct EditFactsRequest {
    chat_id: i64,
    /// The whole list, in order; blank entries are dropped. The goal is not part of it and stays as it is.
    facts: Vec<String>,
}

/// Replaces a chat's key facts with the user's version. The next fold keeps building on it. Refused
/// (409) while the chat has a run going on, and before its first fold, when it has no facts yet.
#[utoipa::path(
    post,
    path = "/api/chats/facts",
    tag = "chats",
    request_body = EditFactsRequest,
    responses(
        (status = 204, description = "Facts saved"),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 409, description = "The chat has a run going on, or no summary yet", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn edit_facts(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<EditFactsRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    services.agent.edit_key_facts(body.chat_id, body.facts).await?;

    Ok(StatusCode::NO_CONTENT)
}
