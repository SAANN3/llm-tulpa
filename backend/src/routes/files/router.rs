use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::download::*;
use super::get::*;
use super::upload::*;

/// Axum caps any extractor that buffers the whole body (`Multipart` fields' `.bytes()`
/// included) at 2MB by default — fine for `chat_id`/`read_only`'s text fields, far too
/// small for the `file` field itself. Raised here rather than app-wide since this is
/// the one route domain that expects large request bodies.
const MAX_UPLOAD_BYTES: usize = 500 * 1024 * 1024;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_file))
        .route("/download", get(download_file))
        .route("/upload", post(upload_file))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_file, download_file, upload_file),
    components(schemas(FileOut, FileListOut, GetFileResponse, UploadFileRequest)),
)]
pub struct ApiDoc;
