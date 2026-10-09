use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct EditSummaryRequest {
    chat_id: i64,
    summary: String,
}

/// Replaces the text of a chat's compaction summary with the user's version; it still covers the
/// same messages. Refused (409) while the chat has a run going on, and before its first fold.
#[utoipa::path(
    post,
    path = "/api/chats/summary",
    tag = "chats",
    request_body = EditSummaryRequest,
    responses(
        (status = 204, description = "Summary saved"),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 409, description = "The chat has a run going on, or no summary yet", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn edit_summary(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<EditSummaryRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;
    services.agent.edit_summary(body.chat_id, body.summary).await?;

    Ok(StatusCode::NO_CONTENT)
}
