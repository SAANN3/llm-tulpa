use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    routes::auth::AuthUser, services::error::ErrorService, services::llm::ThinkingCapability, state::AppState,
};

#[derive(Deserialize, IntoParams)]
pub(crate) struct ThinkingCapabilityQuery {
    /// The chat whose bound model to inspect. Without it, the caller's own default model
    /// is used (the composer on the home page has no chat yet).
    chat_id: Option<i64>,
}

/// What a model actually supports for `think` — a graded set of effort levels
/// (`Graduated`), a plain on/off toggle only (`OnOff`), or nothing at all
/// (`Unsupported`). The model is the given chat's bound one, or the caller's default when
/// no chat is given. Discovered fresh from the model's own chat template on every call
/// (see `OllamaService::thinking_capability`) — nothing here is cached or persisted, so
/// it's always correct for whichever model a chat is bound to right now, with nothing to
/// invalidate when that changes. Meant to be called whenever a caller (the composer's
/// thinking-mode control) needs to know what to offer, not once at startup.
#[utoipa::path(
    get,
    path = "/api/llm/thinking_capability",
    tag = "llm",
    params(ThinkingCapabilityQuery),
    responses(
        (status = 200, description = "Detected thinking capability for the model", body = ThinkingCapability),
        (status = 404, description = "`chat_id` given but no such chat exists", body = crate::services::error::ErrorBody),
        (status = 409, description = "No model has been selected yet", body = crate::services::error::ErrorBody),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn thinking_capability(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ThinkingCapabilityQuery>,
) -> Result<Json<ThinkingCapability>, ErrorService> {
    let services = state.services().await?;
    let model = match query.chat_id {
        Some(chat_id) => services.chat_store.owned_chat(auth.id, chat_id).await?.model,
        None => services.settings_store.effective_model(auth.id).await?,
    };

    let result = state.ollama.thinking_capability(&model).await?;
    Ok(Json(result))
}
