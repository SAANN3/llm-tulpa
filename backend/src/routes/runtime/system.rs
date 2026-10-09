use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{routes::auth::AuthUser, services::{error::ErrorService, system_load::SystemSnapshot}, state::AppState};

/// What the machine is using right now: CPU, memory, the model server's own memory, and the GPU's memory,
/// load, temperature and clock (from the AMD driver on Linux; absent elsewhere). CPU use is measured since
/// the previous call, so a page polls it.
#[utoipa::path(
    get,
    path = "/api/runtime/system",
    tag = "runtime",
    responses((status = 200, description = "A snapshot of the machine's load", body = SystemSnapshot)),
)]
pub async fn system(State(state): State<Arc<AppState>>, _user: AuthUser) -> Result<Json<SystemSnapshot>, ErrorService> {
    let server_pid = state.runtime.loaded_launch().map(|(_, pid)| pid);
    let load = state.system_load.clone();
    // Reads /proc and sysfs: off the async runtime
    let snapshot = tokio::task::spawn_blocking(move || load.snapshot(server_pid))
        .await
        .map_err(|_| ErrorService::internal("reading the system's load failed"))?;
    Ok(Json(snapshot))
}
