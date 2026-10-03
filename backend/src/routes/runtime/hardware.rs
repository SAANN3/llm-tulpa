use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{routes::auth::OwnerUser, services::llama_install::Hardware, state::AppState};

/// Owner-only. What this machine has (CPU, memory, GPUs) and which llama.cpp build suits it, with the
/// libraries that build needs and are missing, each with the command that installs it here.
#[utoipa::path(
    get,
    path = "/api/runtime/hardware",
    tag = "runtime",
    responses(
        (status = 200, description = "The detected hardware and the recommended build", body = Hardware),
        (status = 403, description = "Only the owner can set up the model server", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn hardware(State(_state): State<Arc<AppState>>, _owner: OwnerUser) -> Json<Hardware> {
    // Reads sysfs and may run `nvidia-smi`: off the async runtime
    Json(tokio::task::spawn_blocking(crate::services::llama_install::detect_hardware).await.expect("hardware detection panicked"))
}
