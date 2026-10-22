use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{plugins::tab::TabLayout, routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub struct TabQuery {
    pub plugin_name: String,
    pub plugin_subname: String,
}

/// What a plugin's tab shows the caller now.
#[utoipa::path(
    get,
    path = "/api/plugins/tab",
    tag = "plugins",
    params(TabQuery),
    responses(
        (status = 200, description = "The tab's layout for the caller", body = TabLayout),
        (status = 404, description = "No such tab, or its plugin is not enabled", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn plugin_tab(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<TabQuery>,
) -> Result<Json<TabLayout>, ErrorService> {
    let layout = state.services().await?.plugin_registry.tab_layout(&query.plugin_name, &query.plugin_subname, auth.id).await?;
    Ok(Json(layout))
}
