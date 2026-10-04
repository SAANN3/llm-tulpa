use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Serialize, ToSchema)]
pub(crate) struct ManagedModelOut {
    pub(crate) id: i64,
    /// The file, relative to the model directory
    pub(crate) file: String,
    pub(crate) display_name: Option<String>,
    /// Its launch profiles' ids
    pub(crate) profile_ids: Vec<i64>,
    /// The file is no longer in the model folder (false while no folder is chosen)
    pub(crate) file_missing: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ManagedModelsOut {
    models: Vec<ManagedModelOut>,
}

/// The model files registered to run on the backend's own llama.cpp, with the launch profiles each
/// has. `GET /api/llm/local_files` lists what is on disk; registering one makes it a model.
#[utoipa::path(
    get,
    path = "/api/runtime/models",
    tag = "runtime",
    responses((status = 200, description = "Registered models", body = ManagedModelsOut)),
)]
pub async fn models(State(state): State<Arc<AppState>>, _auth: AuthUser) -> Result<Json<ManagedModelsOut>, ErrorService> {
    let services = state.services().await?;
    let profiles = services.launch_store.list(None).await?;
    let models = services
        .model_store
        .list(crate::facade::launch::MANAGED_PROVIDER)
        .await?
        .into_iter()
        .map(|model| ManagedModelOut {
            profile_ids: profiles.iter().filter(|p| p.model_id == model.id).map(|p| p.id).collect(),
            file_missing: state.library.is_missing(&model.name),
            id: model.id,
            file: model.name,
            display_name: model.display_name,
        })
        .collect();
    Ok(Json(ManagedModelsOut { models }))
}
