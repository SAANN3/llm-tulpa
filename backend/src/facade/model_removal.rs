//! Removing a model the backend runs itself: the database entry and, when asked, the file. The model's
//! launch profiles go with it; the chats bound to it move to what their owner's default is without
//! it, and the sampling presets made for it stay as presets for any model that say where they came from.

use std::sync::Arc;

use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::facade::launch::MANAGED_PROVIDER;
use crate::services::{
    bootstrap::AppServices,
    error::ErrorService,
    llama_runtime::{LlamaRuntime, RuntimeErrors},
    model_library::ModelLibrary,
};

#[derive(Serialize, ToSchema)]
pub struct RemovedModel {
    /// How many chats were moved to another model
    pub chats_moved: u64,
    /// How many sampling presets stay, now for any model
    pub presets_kept: u64,
    /// Whether the file was deleted from the model folder
    pub file_deleted: bool,
}

/// Removes model `id`. With `delete_file` its `.gguf` is deleted as well (a file that is already gone is
/// no reason to refuse); the vision projector it was paired with stays, since another model may use it.
/// Refused while a turn is running on it, and when it is the only model there is (a chat can't have
/// none). An idle model that is loaded is stopped first.
pub async fn remove_model(
    services: &AppServices,
    runtime: &Arc<LlamaRuntime>,
    library: &ModelLibrary,
    id: i64,
    delete_file: bool,
) -> Result<RemovedModel, ErrorService> {
    let model = services
        .model_store
        .get_many(&[id])
        .await?
        .remove(&id)
        .ok_or_else(|| ErrorService::new(StatusCode::NOT_FOUND, "no such model"))?;
    if model.provider != MANAGED_PROVIDER {
        return Err(ErrorService::new(StatusCode::BAD_REQUEST, "only a model the backend runs itself can be removed here"));
    }
    if !services.model_store.others_exist(id).await? {
        return Err(ErrorService::new(StatusCode::CONFLICT, "this is the only model: add another one before removing it, since every chat needs a model"));
    }

    // Stopped first: the running server holds the file, and a turn on it is not ours to cut off
    let status = runtime.status();
    let own_profiles: Vec<i64> = services.launch_store.list(Some(id)).await?.into_iter().map(|p| p.id).collect();
    if status.profile_id.is_some_and(|loaded| own_profiles.contains(&loaded)) {
        if !status.holders.is_empty() {
            return Err(RuntimeErrors::Busy { holders: status.holders.join(", ") }.into());
        }
        runtime.stop().await?;
    }

    // The file goes before the database entry: a failure here leaves everything as it was, and a
    // failure after it leaves an entry whose file is missing, which can be removed again
    let mut file_deleted = false;
    if delete_file && !library.is_missing(&model.name) {
        let path = library.resolve_local(&model.name)?;
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| ErrorService::new(StatusCode::INTERNAL_SERVER_ERROR, format!("could not delete {}: {e}", path.display())))?;
        file_deleted = true;
    }

    let label = model.display_name.clone().unwrap_or_else(|| model.name.clone());
    let presets_kept = services.preset_store.detach_model(id, &label).await?;
    let chats_moved = services.chat_store.move_off_model(id).await?;
    services.model_store.remove(id).await?;

    // Calls that name no launch ran on the default model's profile, which may have been this one
    match services.launches.default_request().await? {
        Some(request) => runtime.set_default(request),
        None => runtime.clear_default(),
    }
    Ok(RemovedModel { chats_moved, presets_kept, file_deleted })
}
