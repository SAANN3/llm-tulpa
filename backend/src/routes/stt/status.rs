use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::AuthUser,
    services::{error::ErrorService, stt::{SttBenchmark, SttModel, SttTask, SttTuning}},
    state::AppState,
};

#[derive(Serialize, ToSchema)]
pub(crate) struct SttStatusOut {
    /// Every model that can be installed, with whether it is
    models: Vec<SttModel>,
    /// The install going on, or the last one that ended
    task: Option<SttTask>,
    /// Whether voice input can be used at all: some model is installed
    ready: bool,
    /// The model the caller's voice input runs on: the one they chose if it is installed, else the first installed; `None`
    /// when nothing is installed
    model: Option<String>,
    /// The language the caller chose for it, `None` for the model to find out
    language: Option<String>,
    /// How the models run (the owner sets it)
    tuning: SttTuning,
    /// How many cores this machine has
    cores: u32,
    /// The measurement of the models' speed going on, or the last one that ended
    benchmark: Option<SttBenchmark>,
}

/// Which speech-to-text models exist and are installed, how an install is going, and what the caller's voice input
/// would use. The page asks this to know whether to enable its microphone button.
#[utoipa::path(
    get,
    path = "/api/stt",
    tag = "stt",
    responses((status = 200, description = "Models and the install", body = SttStatusOut)),
)]
pub async fn status(State(state): State<Arc<AppState>>, auth: AuthUser) -> Result<Json<SttStatusOut>, ErrorService> {
    let (chosen, language) = state.services().await?.settings_store.stt_choices(auth.id).await?;
    let status = state.stt.status();
    let model = state.stt.resolve_model(chosen.as_deref()).ok().map(|m| m.id().to_string());
    Ok(Json(SttStatusOut { ready: model.is_some(), model, language, models: status.models, task: status.task, tuning: status.tuning, cores: status.cores, benchmark: status.benchmark }))
}
