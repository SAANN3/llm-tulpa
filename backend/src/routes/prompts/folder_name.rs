use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::prompt::GreetOut, routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct FolderNameRequest {
    /// Either what the folder is for ("my rust backend chats") or content to
    /// summarize into a name — either way, generated live on this request, not cached.
    content: String,
}

/// A very short (1-4 word) folder name summarizing `content`.
#[utoipa::path(
    post,
    path = "/api/prompts/folder_name",
    tag = "prompts",
    request_body = FolderNameRequest,
    responses(
        (status = 200, description = "Name generated", body = GreetOut),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn folder_name(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<FolderNameRequest>,
) -> Result<Json<GreetOut>, ErrorService> {
    let services = state.services().await?;
    let model = services.settings_store.effective_model(user.id).await?;
    let result = services.prompt.folder_name(body.content, &model).await?;

    Ok(Json(result))
}
