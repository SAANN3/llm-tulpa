use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{facade::prompt::{GreetOut, NameKind}, routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct BlueprintNameRequest {
    /// Description of the automation to summarize into a name.
    content: String,
}

/// A very short (1-4 word) blueprint name summarizing `content`.
#[utoipa::path(
    post,
    path = "/api/prompts/blueprint_name",
    tag = "prompts",
    request_body = BlueprintNameRequest,
    responses(
        (status = 200, description = "Name generated", body = GreetOut),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn blueprint_name(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<BlueprintNameRequest>,
) -> Result<Json<GreetOut>, ErrorService> {
    let services = state.services().await?;
    let model = services.settings_store.effective_model(user.id).await?;
    let result = services.prompt.resource_name(NameKind::Blueprint, body.content, &model).await?;

    Ok(Json(result))
}
