use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::export::{ExportedPreset, FORMAT};
use crate::{
    routes::auth::AuthUser,
    services::{
        error::ErrorService,
        llm::Sampling,
        preset_store::{PresetInput, PresetStoreErrors},
    },
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct ImportPresetsRequest {
    /// The marker of a presets file (`llm-tulpa-sampling-presets`)
    format: String,
    presets: Vec<ExportedPreset>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ImportPresetsOut {
    /// How many presets were added
    imported: usize,
    /// Of those, how many had to be renamed (`name (2)`) because the caller already had one by that name
    renamed: usize,
    /// Of those, how many were made for a model this install doesn't know, and so became any-model presets
    any_model: usize,
}

/// Adds the presets of a file made by `GET /api/presets/export` to the caller's own. A preset whose
/// model isn't known here becomes an any-model preset, and one whose name is taken gets a number
/// after it, so nothing already there is overwritten.
#[utoipa::path(
    post,
    path = "/api/presets/import",
    tag = "presets",
    request_body = ImportPresetsRequest,
    responses(
        (status = 200, description = "Presets imported", body = ImportPresetsOut),
        (status = 400, description = "Not a presets file, or a value is out of range", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn import_presets(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<ImportPresetsRequest>,
) -> Result<Json<ImportPresetsOut>, ErrorService> {
    if body.format != FORMAT {
        return Err(ErrorService::new(StatusCode::BAD_REQUEST, "that is not a sampling presets file"));
    }
    if body.presets.len() > 200 {
        return Err(ErrorService::new(StatusCode::BAD_REQUEST, "a presets file holds at most 200 presets"));
    }

    let services = state.services().await?;
    let (mut imported, mut renamed, mut any_model) = (0, 0, 0);
    for exported in body.presets {
        let model_id = match &exported.model {
            Some(model) => services.model_store.find(&model.provider, &model.name).await?.map(|m| m.id),
            None => None,
        };
        if exported.model.is_some() && model_id.is_none() {
            any_model += 1;
        }

        let base = PresetInput {
            model_id,
            name: exported.name.clone(),
            sampling: Sampling {
                temperature: exported.temperature,
                top_p: exported.top_p,
                top_k: exported.top_k,
                min_p: exported.min_p,
                repeat_penalty: exported.repeat_penalty,
                presence_penalty: exported.presence_penalty,
                seed: exported.seed,
            },
        };

        // The name is the user's to keep unique; a taken one gets a number rather than failing the import
        let mut attempt = 1;
        loop {
            let name = if attempt == 1 { base.name.clone() } else { format!("{} ({attempt})", base.name) };
            match services.preset_store.create(auth.id, PresetInput { name, ..base.clone() }).await {
                Ok(_) => break,
                Err(PresetStoreErrors::Duplicate) if attempt < 50 => attempt += 1,
                Err(e) => return Err(e.into()),
            }
        }
        imported += 1;
        if attempt > 1 {
            renamed += 1;
        }
    }
    Ok(Json(ImportPresetsOut { imported, renamed, any_model }))
}
