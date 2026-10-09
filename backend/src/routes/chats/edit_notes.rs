use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct EditNotesRequest {
    chat_id: i64,
    /// Absent or blank clears the notes
    notes: Option<String>,
}

/// Replaces the model's working notes in a chat with the user's version, notes still waiting for the
/// next fold included; the model is sent them from its next turn. Refused (409) while the chat has a
/// run going on.
#[utoipa::path(
    post,
    path = "/api/chats/notes",
    tag = "chats",
    request_body = EditNotesRequest,
    responses(
        (status = 204, description = "Notes saved"),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 409, description = "The chat has a run going on", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn edit_notes(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<EditNotesRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    services.agent.edit_notes(body.chat_id, body.notes).await?;

    Ok(StatusCode::NO_CONTENT)
}
