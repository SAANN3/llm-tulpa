use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    config,
    routes::auth::OwnerUser,
    services::{error::ErrorService, stt::SttTuning},
    state::AppState,
};

/// Owner-only. Changes how many CPU threads the speech models read with and how long an idle model stays loaded. Both
/// apply at once, without a restart; they are kept in `settings.json`.
#[utoipa::path(
    post,
    path = "/api/stt/tuning",
    tag = "stt",
    request_body = SttTuning,
    responses(
        (status = 200, description = "Saved", body = SttTuning),
        (status = 400, description = "A value is out of range", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_tuning(State(state): State<Arc<AppState>>, _owner: OwnerUser, Json(body): Json<SttTuning>) -> Result<Json<SttTuning>, ErrorService> {
    body.validate()?;
    {
        let mut cfg = state.config.write().await;
        cfg.stt_threads = body.threads;
        cfg.stt_idle_unload_minutes = body.idle_unload_minutes;
        config::save(&cfg).map_err(|e| {
            tracing::error!("could not save config: {e}");
            ErrorService::internal("the settings are in use, but could not be saved to settings.json")
        })?;
    }
    state.stt.set_tuning(body.clone());
    Ok(Json(body))
}
