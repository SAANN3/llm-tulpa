use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

/// The marker a presets file carries, so an import can tell it from any other JSON.
pub(super) const FORMAT: &str = "llm-tulpa-sampling-presets";

/// The model a preset was made for, by identity rather than id, so the file works on another install.
#[derive(Serialize, serde::Deserialize, ToSchema, Clone)]
pub(crate) struct PresetModel {
    pub(crate) provider: String,
    pub(crate) name: String,
}

#[derive(Serialize, serde::Deserialize, ToSchema, Clone)]
pub(crate) struct ExportedPreset {
    pub(crate) name: String,
    pub(crate) model: Option<PresetModel>,
    pub(crate) temperature: Option<f32>,
    pub(crate) top_p: Option<f32>,
    pub(crate) top_k: Option<i32>,
    pub(crate) min_p: Option<f32>,
    pub(crate) repeat_penalty: Option<f32>,
    pub(crate) presence_penalty: Option<f32>,
    pub(crate) seed: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct PresetsFile {
    format: &'static str,
    version: u32,
    presets: Vec<ExportedPreset>,
}

/// All of the caller's presets as a file they can keep or hand to someone, to bring back with
/// `POST /api/presets/import`.
#[utoipa::path(
    get,
    path = "/api/presets/export",
    tag = "presets",
    responses((status = 200, description = "The caller's presets", body = PresetsFile)),
)]
pub async fn export_presets(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<PresetsFile>, ErrorService> {
    let services = state.services().await?;
    let presets = services.preset_store.list(auth.id, None).await?;

    let mut model_ids: Vec<i64> = presets.iter().filter_map(|p| p.model_id).collect();
    model_ids.sort_unstable();
    model_ids.dedup();
    let models = services.model_store.get_many(&model_ids).await?;

    let presets = presets
        .into_iter()
        .map(|p| {
            let model = p
                .model_id
                .and_then(|id| models.get(&id))
                .map(|m| PresetModel { provider: m.provider.clone(), name: m.name.clone() });
            ExportedPreset {
                name: p.name,
                model,
                temperature: p.sampling.temperature,
                top_p: p.sampling.top_p,
                top_k: p.sampling.top_k,
                min_p: p.sampling.min_p,
                repeat_penalty: p.sampling.repeat_penalty,
                presence_penalty: p.sampling.presence_penalty,
                seed: p.sampling.seed,
            }
        })
        .collect();
    Ok(Json(PresetsFile { format: FORMAT, version: 1, presets }))
}
