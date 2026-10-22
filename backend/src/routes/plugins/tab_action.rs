use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    plugins::tab::{TabActionBody, TabLayout},
    routes::auth::AuthUser,
    services::error::ErrorService,
    state::AppState,
};

/// What the caller did in a plugin's tab: a control's action and what it sent. Answers with the layout as it is after.
#[utoipa::path(
    post,
    path = "/api/plugins/tab/action",
    tag = "plugins",
    request_body = TabActionBody,
    responses(
        (status = 200, description = "The tab's layout after the action", body = TabLayout),
        (status = 400, description = "The plugin refused the action", body = crate::services::error::ErrorBody),
        (status = 404, description = "No such tab, or its plugin is not enabled", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn plugin_tab_action(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<TabActionBody>,
) -> Result<Json<TabLayout>, ErrorService> {
    let layout = state
        .services()
        .await?
        .plugin_registry
        .tab_action(&body.plugin_name, &body.plugin_subname, auth.id, &body.action, body.args)
        .await?;
    Ok(Json(layout))
}
