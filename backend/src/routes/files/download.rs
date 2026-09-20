use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::header,
    response::IntoResponse,
};
use serde::Deserialize;
use utoipa::IntoParams;

use axum::http::StatusCode;

use crate::{routes::auth::AuthUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub(crate) struct DownloadFileQuery {
    id: i64,
}

/// Streams a file's actual bytes back, `Content-Disposition: attachment` with its
/// UI-facing name, so hitting this URL directly (e.g. a browser navigation, an `<a
/// href>`) saves it instead of trying to render it inline. `Content-Type` is guessed
/// from the file's own bytes (same sniffing `storage.detect_file_type` uses), falling
/// back to `application/octet-stream` for anything unrecognized.
#[utoipa::path(
    get,
    path = "/api/files/download",
    tag = "files",
    params(DownloadFileQuery),
    responses(
        (status = 200, description = "The file's raw bytes", content_type = "application/octet-stream"),
        (status = 404, description = "No such file", body = crate::services::error::ErrorBody),
        (status = 500, description = "Database query failed, or the file couldn't be read from disk", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn download_file(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<DownloadFileQuery>,
) -> Result<impl IntoResponse, ErrorService> {
    let record = state.services().await?.file_store.get(query.id).await?;
    if record.user_id != auth.id {
        return Err(ErrorService::new(StatusCode::NOT_FOUND, "no such file"));
    }

    let bytes = tokio::fs::read(&record.full_path)
        .await
        .map_err(|e| ErrorService::internal(format!("couldn't read '{}': {e}", record.full_path)))?;

    let content_type = infer::get(&bytes)
        .map(|kind| kind.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // Quotes stripped rather than escaped — a stray `"` in a UI-chosen name breaking
    // the header's quoting is worse than losing the character.
    let disposition = format!("attachment; filename=\"{}\"", record.file_name.replace('"', "'"));

    let headers = [(header::CONTENT_TYPE, content_type), (header::CONTENT_DISPOSITION, disposition)];

    Ok((headers, bytes))
}
