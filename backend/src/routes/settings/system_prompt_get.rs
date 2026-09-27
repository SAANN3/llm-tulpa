use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    facade::agent::default_system_prompt,
    routes::auth::AuthUser,
    services::{error::ErrorService, settings_store::SystemPromptOut},
    state::AppState,
};

/// Reads the authenticated user's custom system prompt, alongside the built-in default the
/// settings page shows next to it.
#[utoipa::path(
    get,
    path = "/api/settings/system-prompt",
    tag = "settings",
    responses(
        (status = 200, description = "The user's custom system prompt (null while the default applies) and the built-in default", body = SystemPromptOut),
        (status = 404, description = "Settings have not been configured yet", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_system_prompt(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<SystemPromptOut>, ErrorService> {
    let custom = state.services().await?.settings_store.system_prompt(auth.id).await?;

    Ok(Json(SystemPromptOut {
        custom,
        default: default_system_prompt(),
    }))
}
