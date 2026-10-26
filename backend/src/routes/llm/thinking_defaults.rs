use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct ThinkingDefaultOut {
    provider: String,
    model: String,
    /// `off`, `on` or one of the model's own levels
    level: String,
}

/// The thinking level each of the caller's models starts on in a chat that has none of its own. A model with no entry starts
/// on its first level.
#[utoipa::path(
    get,
    path = "/api/llm/thinking_defaults",
    tag = "llm",
    responses(
        (status = 200, description = "The caller's chosen levels", body = Vec<ThinkingDefaultOut>),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn thinking_defaults(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<Vec<ThinkingDefaultOut>>, ErrorService> {
    let services = state.services().await?;
    let defaults = services.model_store.thinking_defaults(auth.id).await?;
    Ok(Json(defaults.into_iter().map(|(model, level)| ThinkingDefaultOut { provider: model.provider, model: model.name, level }).collect()))
}
