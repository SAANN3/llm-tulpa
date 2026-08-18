use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::prompt::GreetOut, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct ChatNameRequest {
    content: String,
}

/// A very short (1-5 word) label summarizing `content`, written from the user's own
/// perspective — meant to be used as a chat/entry name. Generated live on this request,
/// not cached.
#[utoipa::path(
    post,
    path = "/api/prompts/chat_name",
    tag = "prompts",
    request_body = ChatNameRequest,
    responses(
        (status = 200, description = "Name generated", body = GreetOut),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn chat_name(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ChatNameRequest>,
) -> Result<Json<GreetOut>, ErrorService> {
    let result = state.prompt.chat_name(body.content).await?;

    Ok(Json(result))
}
