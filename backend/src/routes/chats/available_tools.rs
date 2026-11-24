use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::{error::ErrorService, tools::ToolCatalogEntry}, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct AvailableToolsOut {
    tools: Vec<ToolCatalogEntry>,
}

/// Every tool this user can be sent: the built-in ones and the ones their plugins have published.
#[utoipa::path(
    get,
    path = "/api/chats/available_tools",
    tag = "chats",
    responses(
        (status = 200, description = "The tools this user can choose from", body = AvailableToolsOut),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn available_tools(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<AvailableToolsOut>, ErrorService> {
    let tools = state
        .tools
        .catalog(auth.id)
        .await;
    Ok(Json(AvailableToolsOut { tools }))
}
