use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub struct GetHelpQuery {
    pub plugin_name: String,
    pub plugin_subname: String,
}

/// A plugin's own info/help message — how to use it, step by step. Present for every
/// registered plugin, even one that hasn't been configured yet (comes from the
/// builder, not the live instance — same reasoning as `plugin_settings_schema`).
#[utoipa::path(
    get,
    path = "/api/plugins/help",
    tag = "plugins",
    params(GetHelpQuery),
    responses(
        (status = 200, description = "This plugin's help message", body = String),
        (status = 404, description = "No such plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn plugin_help(State(state): State<Arc<AppState>>, Query(query): Query<GetHelpQuery>) -> Result<Json<String>, ErrorService> {
    let builder = state
        .plugin_registry
        .builder(&query.plugin_name, &query.plugin_subname)
        .await
        .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "no such plugin"))?;

    Ok(Json(builder.help_message()))
}
