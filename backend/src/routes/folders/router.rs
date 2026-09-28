use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::create::*;
use super::delete::*;
use super::get::*;
use super::rename::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_folders).post(create_folder).delete(delete_folder))
        .route("/rename", post(rename_folder))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_folders, create_folder, delete_folder, rename_folder),
    components(schemas(FolderOut, FoldersOut, GetFoldersResponse, CreateFolderRequest, RenameFolderRequest)),
)]
pub struct ApiDoc;
