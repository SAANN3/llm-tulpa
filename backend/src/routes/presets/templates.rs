use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use super::dto::PresetIn;
use crate::{routes::auth::AuthUser, services::preset_store};

#[derive(Serialize, ToSchema)]
pub(crate) struct TemplatesOut {
    templates: Vec<PresetIn>,
}

/// Built-in starting points to copy into the caller's own presets (post one to `/api/presets`).
/// Common shapes, not the values any model's author recommends.
#[utoipa::path(
    get,
    path = "/api/presets/templates",
    tag = "presets",
    responses((status = 200, description = "The built-in templates", body = TemplatesOut)),
)]
pub async fn preset_templates(_auth: AuthUser) -> Json<TemplatesOut> {
    Json(TemplatesOut { templates: preset_store::templates().into_iter().map(Into::into).collect() })
}
