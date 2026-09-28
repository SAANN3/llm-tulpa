use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use sea_orm::prelude::DateTimeUtc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct GetFoldersQuery {
    /// A specific folder's id. Given alone, the response is that folder's info instead
    /// of a list.
    id: Option<i64>,
    /// Case-insensitive substring filter on the folder name.
    q: Option<String>,
    limit: Option<u64>,
    skip: Option<u64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FolderOut {
    pub(crate) id: i64,
    pub(crate) name: String,
    #[schema(value_type = String, format = "date-time")]
    pub(crate) created_at: DateTimeUtc,
    #[schema(value_type = String, format = "date-time")]
    pub(crate) updated_at: DateTimeUtc,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FoldersOut {
    folders: Vec<FolderOut>,
    total: u64,
}

/// `id` present → a single folder's info. `id` absent → the paginated folder list.
/// Same shape as `GET /api/chats`.
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum GetFoldersResponse {
    Single(FolderOut),
    List(FoldersOut),
}

/// `id` given → that folder's info (404 if it doesn't exist). `id` omitted → a page of the
/// caller's folders, ordered by their most recently active chat, plus a total count for
/// pagination.
#[utoipa::path(
    get,
    path = "/api/folders",
    tag = "folders",
    params(GetFoldersQuery),
    responses(
        (status = 200, description = "A single folder, or a page of folders", body = GetFoldersResponse),
        (status = 404, description = "`id` given but no such folder exists", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_folders(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<GetFoldersQuery>,
) -> Result<Json<GetFoldersResponse>, ErrorService> {
    let services = state.services().await?;

    if let Some(id) = query.id {
        let folder = services.folder_store.owned_folder(auth.id, id).await?;
        return Ok(Json(GetFoldersResponse::Single(FolderOut {
            id: folder.id,
            name: folder.name,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
        })));
    }

    let limit = query.limit.unwrap_or(50);
    let skip = query.skip.unwrap_or(0);
    let (folders, total) = services.folder_store.folders(auth.id, query.q.as_deref(), limit, skip).await?;

    let folders = folders
        .into_iter()
        .map(|folder| FolderOut {
            id: folder.id,
            name: folder.name,
            created_at: folder.created_at,
            updated_at: folder.updated_at,
        })
        .collect();

    Ok(Json(GetFoldersResponse::List(FoldersOut { folders, total })))
}
