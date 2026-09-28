use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetFolderRequest {
    chat_id: i64,
    /// The folder to move the chat into, or `null` to ungroup it.
    folder_id: Option<i64>,
}

/// Moves a chat into a folder, or out of one (`folder_id: null`).
#[utoipa::path(
    post,
    path = "/api/chats/folder",
    tag = "chats",
    request_body = SetFolderRequest,
    responses(
        (status = 204, description = "Folder set"),
        (status = 404, description = "No such chat, or no such folder", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_folder(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SetFolderRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.set_folder(auth.id, body.chat_id, body.folder_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
