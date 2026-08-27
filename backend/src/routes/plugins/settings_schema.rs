use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{services::error::ErrorService, state::AppState, tools::base::PropertyInfo};

#[derive(Deserialize, IntoParams)]
pub struct SettingsSchemaQuery {
    pub plugin_name: String,
    pub plugin_subname: String,
}

/// A plugin's settings schema, in the same shape used for tool-calling args — what the
/// frontend renders its settings form from. Present for every registered plugin, even
/// one that hasn't been configured yet (the schema comes from the builder, not the
/// live instance).
#[utoipa::path(
    get,
    path = "/api/plugins/settings_schema",
    tag = "plugins",
    params(SettingsSchemaQuery),
    responses(
        (status = 200, description = "This plugin's settings schema", body = Vec<PropertyInfo>),
        (status = 404, description = "No such plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn plugin_settings_schema(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SettingsSchemaQuery>,
) -> Result<Json<Vec<PropertyInfo>>, ErrorService> {
    let builder = state
        .plugin_registry
        .builder(&query.plugin_name, &query.plugin_subname)
        .await
        .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "no such plugin"))?;

    Ok(Json(builder.settings_schema()))
}
