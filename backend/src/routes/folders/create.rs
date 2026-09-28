use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

use super::get::FolderOut;

#[derive(Deserialize, ToSchema)]
pub(crate) struct CreateFolderRequest {
    name: String,
}

/// Creates a new folder with the given name and returns its info.
#[utoipa::path(
    post,
    path = "/api/folders",
    tag = "folders",
    request_body = CreateFolderRequest,
    responses(
        (status = 200, description = "Folder created", body = FolderOut),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn create_folder(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<CreateFolderRequest>,
) -> Result<Json<FolderOut>, ErrorService> {
    let services = state.services().await?;
    let folder = services.folder_store.create_folder(auth.id, body.name).await?;

    Ok(Json(FolderOut {
        id: folder.id,
        name: folder.name,
        created_at: folder.created_at,
        updated_at: folder.updated_at,
    }))
}
