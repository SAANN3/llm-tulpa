use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// The longest level a request may name; the models' own are a few characters.
const MAX_LEVEL_CHARS: usize = 32;

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetThinkingDefaultRequest {
    model: String,
    /// The provider of `model`; llama.cpp when left out
    provider: Option<String>,
    /// `off`, `on` or one of the model's own levels; leave out to take the choice away
    level: Option<String>,
}

/// Chooses the thinking level a model starts on for the caller in a chat that has none of its own, or, with no `level`, takes
/// the choice away. The level is not checked against the model's template here: the composer offers only the model's own, and
/// a level the model no longer has is skipped when the composer reads it.
#[utoipa::path(
    post,
    path = "/api/llm/thinking_default",
    tag = "llm",
    request_body = SetThinkingDefaultRequest,
    responses(
        (status = 204, description = "Level chosen, or the choice removed"),
        (status = 400, description = "An empty or overlong level", body = crate::services::error::ErrorBody),
        (status = 404, description = "The model isn't registered", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_thinking_default(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SetThinkingDefaultRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    let provider = body.provider.as_deref().unwrap_or(crate::facade::launch::MANAGED_PROVIDER);
    let model = services
        .model_store
        .find(provider, &body.model)
        .await?
        .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "no such model"))?;

    match body.level.as_deref().map(str::trim) {
        None => services.model_store.clear_thinking_default(auth.id, model.id).await?,
        Some(level) if level.is_empty() || level.chars().count() > MAX_LEVEL_CHARS => {
            return Err(ErrorService::new(StatusCode::BAD_REQUEST, "the level must be a short name"));
        }
        Some(level) => services.model_store.set_thinking_default(auth.id, model.id, level).await?,
    }

    Ok(StatusCode::NO_CONTENT)
}
