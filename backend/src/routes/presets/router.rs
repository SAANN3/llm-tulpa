use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::choose::*;
use super::create::*;
use super::delete::*;
use super::dto::*;
use super::export::*;
use super::get::*;
use super::import::*;
use super::templates::*;
use super::update::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_presets).post(create_preset).delete(delete_preset))
        .route("/update", post(update_preset))
        .route("/choose", post(choose_preset))
        .route("/templates", get(preset_templates))
        .route("/export", get(export_presets))
        .route("/import", post(import_presets))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_presets, create_preset, update_preset, delete_preset, choose_preset, preset_templates, export_presets, import_presets),
    components(schemas(
        PresetOut,
        PresetIn,
        PresetsOut,
        UpdatePresetRequest,
        ChoosePresetRequest,
        TemplatesOut,
        PresetModel,
        ExportedPreset,
        PresetsFile,
        ImportPresetsRequest,
        ImportPresetsOut,
    )),
)]
pub struct ApiDoc;
