use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, settings_store::SettingsUpdate},
    state::AppState,
};

/// Applies a partial update to the authenticated user's settings — only the provided
/// fields are written. Used both by the settings page (a full save) and the setup
/// wizard's incremental per-step saves.
#[utoipa::path(
    post,
    path = "/api/settings",
    tag = "settings",
    request_body = SettingsUpdate,
    responses(
        (status = 204, description = "Settings saved"),
        (status = 400, description = "Timezone offset out of range, or a model that isn't installed", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<SettingsUpdate>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;

    if let Some(model) = &body.active_model {
        let provider = match &body.llm_provider {
            Some(provider) => provider.clone(),
            None => services.settings_store.settings(auth.id).await?.llm_provider,
        };
        state.require_installed_model(&services, &provider, model).await?;
    }

    // Name and timezone are what the greeting and placeholders are written from. Changing the
    // default model deliberately doesn't regenerate them: what's cached still reads fine, and
    // it's replaced by itself when it ages out.
    let affects_generated_content = body.name.is_some() || body.timezone.is_some();
    services.settings_store.update(auth.id, body).await?;

    // The greeting and placeholders were generated from the old name/timezone.
    if affects_generated_content {
        services.user_cache.invalidate(auth.id);
        services.user_cache.warm(auth.id);
    }

    Ok(StatusCode::NO_CONTENT)
}
