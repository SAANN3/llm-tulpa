use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct DeleteFolderQuery {
    id: i64,
}

/// Deletes a folder. Its chats aren't deleted — they're just ungrouped
/// (`chats.folder_id` becomes `null` at the database level).
#[utoipa::path(
    delete,
    path = "/api/folders",
    tag = "folders",
    params(DeleteFolderQuery),
    responses(
        (status = 204, description = "Folder deleted"),
        (status = 404, description = "No such folder", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn delete_folder(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<DeleteFolderQuery>,
) -> Result<StatusCode, ErrorService> {
    let services = state.services().await?;
    services.folder_store.owned_folder(auth.id, query.id).await?;
    services.folder_store.delete_folder(query.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
