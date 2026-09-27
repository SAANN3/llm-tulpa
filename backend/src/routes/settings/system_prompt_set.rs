use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, settings_store::SystemPromptUpdate},
    state::AppState,
};

/// Sets the authenticated user's custom system prompt, or resets it to the built-in default
/// when the prompt is `null`. Takes effect from the user's next turn.
#[utoipa::path(
    post,
    path = "/api/settings/system-prompt",
    tag = "settings",
    request_body = SystemPromptUpdate,
    responses(
        (status = 204, description = "System prompt saved"),
        (status = 400, description = "The prompt is too long", body = crate::services::error::ErrorBody),
        (status = 404, description = "Settings have not been configured yet", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_system_prompt(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SystemPromptUpdate>,
) -> Result<StatusCode, ErrorService> {
    state
        .services()
        .await?
        .settings_store
        .set_system_prompt(auth.id, body.prompt)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
