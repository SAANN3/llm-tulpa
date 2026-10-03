use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::OwnerUser,
    services::llama_install::{InstallTask, Installed},
    state::AppState,
};

#[derive(Serialize, ToSchema)]
pub(crate) struct InstallStatusOut {
    /// What is installed now, `null` when nothing is
    installed: Option<Installed>,
    /// The latest install attempt, `null` when none was made since the backend started
    task: Option<InstallTask>,
}

/// Owner-only. What llama.cpp release is installed and how the running install is going; poll it
/// while `task.state` is `running`.
#[utoipa::path(
    get,
    path = "/api/runtime/install",
    tag = "runtime",
    responses((status = 200, description = "Install state", body = InstallStatusOut)),
)]
pub async fn install_status(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<InstallStatusOut> {
    Json(InstallStatusOut { installed: state.installer.installed(), task: state.installer.task() })
}
