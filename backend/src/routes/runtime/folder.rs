use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    config,
    routes::auth::OwnerUser,
    services::{
        error::ErrorService,
        model_folder::{self, Browsed},
    },
    state::AppState,
};

#[derive(Serialize, ToSchema)]
pub(crate) struct FolderOut {
    /// The model folder, `null` until one is chosen
    path: Option<String>,
    /// Whether files can be created in it, which downloads need
    writable: Option<bool>,
    /// Registered models whose file is not in the folder (after a change of folder)
    missing_models: Vec<String>,
    /// How many bytes can still be written on the disk the folder is on, when the system says
    free_bytes: Option<u64>,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SetFolderRequest {
    /// An absolute path to an existing folder
    path: String,
}

#[derive(Deserialize, IntoParams)]
pub(crate) struct BrowseQuery {
    /// The folder to list; the model folder's parent, else the home folder, when left out
    path: Option<String>,
}

async fn describe(state: &AppState) -> Result<FolderOut, ErrorService> {
    let Some(path) = state.model_folder.get() else {
        return Ok(FolderOut { path: None, writable: None, missing_models: Vec::new(), free_bytes: None });
    };
    let checked = model_folder::check(&path.display().to_string()).ok();
    let missing_models = match state.services().await {
        Ok(services) => services
            .model_store
            .list(crate::facade::launch::MANAGED_PROVIDER)
            .await?
            .into_iter()
            .map(|m| m.name)
            .filter(|name| !path.join(name).is_file())
            .collect(),
        // No database yet (the wizard before its database step): nothing is registered
        Err(_) => Vec::new(),
    };
    let free_bytes = tokio::task::spawn_blocking({
        let path = path.clone();
        move || model_folder::free_bytes(&path)
    })
    .await
    .ok()
    .flatten();
    Ok(FolderOut { path: Some(path.display().to_string()), writable: checked.map(|c| c.writable), missing_models, free_bytes })
}

/// Owner-only. The folder the model files live in, and which registered models aren't in it.
#[utoipa::path(
    get,
    path = "/api/runtime/folder",
    tag = "runtime",
    responses((status = 200, description = "The model folder", body = FolderOut)),
)]
pub async fn get_folder(State(state): State<Arc<AppState>>, _owner: OwnerUser) -> Result<Json<FolderOut>, ErrorService> {
    Ok(Json(describe(&state).await?))
}

/// Owner-only. Chooses the folder the model files live in, from now on, without a restart: models
/// are run from it and Hugging Face downloads land in it. The choice is kept in `settings.json`.
/// Inside Docker only folders the container can see are possible (the mounted `/models`, `/home`).
#[utoipa::path(
    post,
    path = "/api/runtime/folder",
    tag = "runtime",
    request_body = SetFolderRequest,
    responses(
        (status = 200, description = "Chosen; `missing_models` lists registered models not found in it", body = FolderOut),
        (status = 400, description = "Not an existing, readable, absolute folder", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_folder(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Json(body): Json<SetFolderRequest>,
) -> Result<Json<FolderOut>, ErrorService> {
    let checked = model_folder::check(&body.path)?;
    state.model_folder.set(checked.path.clone());
    {
        let mut cfg = state.config.write().await;
        cfg.model_dir = Some(checked.path);
        config::save(&cfg).map_err(|e| {
            tracing::error!("could not save config: {e}");
            ErrorService::internal("the folder is in use, but it could not be saved to settings.json")
        })?;
    }
    Ok(Json(describe(&state).await?))
}

/// Owner-only. The sub-folders of a folder, for choosing the model folder.
#[utoipa::path(
    get,
    path = "/api/runtime/folder/browse",
    tag = "runtime",
    params(BrowseQuery),
    responses(
        (status = 200, description = "The folders inside", body = Browsed),
        (status = 400, description = "No such folder", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn browse_folders(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    Query(query): Query<BrowseQuery>,
) -> Result<Json<Browsed>, ErrorService> {
    let start = match query.path {
        Some(path) => path,
        None => state
            .model_folder
            .get()
            .and_then(|p| p.parent().map(|p| p.display().to_string()))
            .or_else(|| dirs::home_dir().map(|p| p.display().to_string()))
            .unwrap_or_else(|| "/".to_string()),
    };
    Ok(Json(model_folder::browse(&start)?))
}
