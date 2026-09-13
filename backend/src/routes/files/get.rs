use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{services::error::ErrorService, services::file_store::FileRecord, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct GetFileQuery {
    /// A specific file's id. Given alone, the response is that file's metadata instead
    /// of a list.
    id: Option<i64>,
    /// Every file recorded for this chat. Ignored if `id` is also given; one of the
    /// two is required.
    chat_id: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FileOut {
    pub(crate) id: i64,
    /// `null` for a file uploaded before a chat existed to attach it to yet.
    pub(crate) chat_id: Option<i64>,
    pub(crate) full_path: String,
    pub(crate) file_name: String,
    pub(crate) read_only: bool,
}

impl From<FileRecord> for FileOut {
    fn from(record: FileRecord) -> Self {
        Self {
            id: record.id,
            chat_id: record.chat_id,
            full_path: record.full_path,
            file_name: record.file_name,
            read_only: record.read_only,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct FileListOut {
    files: Vec<FileOut>,
}

/// `id` present → a single file's metadata. `id` absent → every file recorded for
/// `chat_id`. Different shapes for the same route rather than two routes, same
/// convention `GET /chats` already uses.
#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum GetFileResponse {
    Single(FileOut),
    List(FileListOut),
}

/// `id` given → that file's metadata (404 if it doesn't exist) — for its actual bytes,
/// see `GET /files/download`. `id` omitted, `chat_id` given → every file recorded for
/// that chat, in no particular order. Neither given → 400.
#[utoipa::path(
    get,
    path = "/api/files",
    tag = "files",
    params(GetFileQuery),
    responses(
        (status = 200, description = "A single file's metadata, or every file recorded for a chat", body = GetFileResponse),
        (status = 400, description = "Neither `id` nor `chat_id` was given", body = crate::services::error::ErrorBody),
        (status = 404, description = "`id` given but no such file exists", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn get_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GetFileQuery>,
) -> Result<Json<GetFileResponse>, ErrorService> {
    if let Some(id) = query.id {
        let record = state.file_store.get(id).await?;
        return Ok(Json(GetFileResponse::Single(record.into())));
    }

    let chat_id = query
        .chat_id
        .ok_or_else(|| ErrorService::new(StatusCode::BAD_REQUEST, "either 'id' or 'chat_id' is required"))?;

    let records = state.file_store.list_by_chat(chat_id).await?;
    let files = records.into_iter().map(FileOut::from).collect();

    Ok(Json(GetFileResponse::List(FileListOut { files })))
}
