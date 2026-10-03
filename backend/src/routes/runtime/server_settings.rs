use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    config,
    routes::auth::OwnerUser,
    services::{error::ErrorService, llama_runtime::Tuning},
    state::AppState,
};

/// Owner-only. How the model server behaves over time: how long it may sit idle before it unloads (and
/// frees the GPU), whether the default model loads when the backend starts, and how long a load may take.
#[utoipa::path(
    get,
    path = "/api/runtime/server",
    tag = "runtime",
    responses((status = 200, description = "The current settings", body = Tuning)),
)]
pub async fn get_server_settings(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<Tuning> {
    Json(state.runtime.tuning())
}

/// Owner-only. Changes them. The idle time and the load timeout apply at once, without a restart; the
/// autostart choice applies from the next start of the backend. Kept in `settings.json`.
#[utoipa::path(
    post,
    path = "/api/runtime/server",
    tag = "runtime",
    request_body = Tuning,
    responses(
        (status = 200, description = "Saved", body = Tuning),
        (status = 400, description = "A value is out of range", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_server_settings(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<Tuning>,
) -> Result<Json<Tuning>, ErrorService> {
    body.validate()?;
    {
        let mut cfg = state.config.write().await;
        cfg.llama_cpp.idle_unload_minutes = body.idle_unload_minutes;
        cfg.llama_cpp.autostart = body.autostart;
        cfg.llama_cpp.load_timeout_secs = body.load_timeout_secs;
        config::save(&cfg).map_err(|e| {
            tracing::error!("could not save config: {e}");
            ErrorService::internal("the settings are in use, but could not be saved to settings.json")
        })?;
    }
    state.runtime.set_tuning(body.clone());
    Ok(Json(body))
}
