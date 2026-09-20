use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// The provider a model is registered under when the request doesn't name one.
const DEFAULT_PROVIDER: &str = "ollama";

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetModelRequest {
    chat_id: i64,
    /// The model this chat should use from now on. Must be installed (pull or import it
    /// first); it's registered in the database if it isn't known yet.
    model: String,
    /// The provider the model belongs to; defaults to `ollama`.
    provider: Option<String>,
}

/// Rebinds a chat to another model. Takes effect on the chat's next turn — the model is
/// read from the chat on every call (see `Agent::advance`), so there's nothing to reload.
#[utoipa::path(
    post,
    path = "/api/chats/model",
    tag = "chats",
    request_body = SetModelRequest,
    responses(
        (status = 204, description = "Model set"),
        (status = 400, description = "Unknown provider, or a model that isn't installed", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_model(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SetModelRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.chat_store.owned_chat(auth.id, body.chat_id).await?;

    let provider = body.provider.as_deref().unwrap_or(DEFAULT_PROVIDER);
    state.require_installed_model(&services, provider, &body.model).await?;
    let model = services.model_store.ensure(provider, &body.model).await?;
    services.chat_store.set_model(body.chat_id, model.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
