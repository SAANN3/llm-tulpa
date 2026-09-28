use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, ToSchema)]
pub(crate) struct RenameFolderRequest {
    folder_id: i64,
    name: String,
}

/// Renames a folder.
#[utoipa::path(
    post,
    path = "/api/folders/rename",
    tag = "folders",
    request_body = RenameFolderRequest,
    responses(
        (status = 204, description = "Folder renamed"),
        (status = 404, description = "No such folder", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn rename_folder(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(body): Json<RenameFolderRequest>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.folder_store.owned_folder(auth.id, body.folder_id).await?;
    services.folder_store.rename_folder(body.folder_id, body.name).await?;

    Ok(StatusCode::NO_CONTENT)
}
