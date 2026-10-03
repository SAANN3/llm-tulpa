use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::download::*;
use super::files::*;
use super::search::*;
use super::tasks::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/search", get(search))
        .route("/files", get(files))
        .route("/download", post(download))
        .route("/tasks", get(tasks))
}

#[derive(OpenApi)]
#[openapi(
    paths(search, files, download, tasks),
    components(schemas(
        SearchOut,
        FilesOut,
        DownloadRequest,
        crate::services::hf_library::HfRepo,
        crate::services::hf_library::HfFile,
        crate::services::hf_library::HfTask,
    )),
)]
pub struct ApiDoc;
