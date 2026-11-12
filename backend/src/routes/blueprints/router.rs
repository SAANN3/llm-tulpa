use super::{create, delete, get as read, list, update};
use crate::state::AppState;
use axum::{Router, routing::get};
use std::sync::Arc;
use utoipa::OpenApi;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(list::list).post(create::create))
        .route(
            "/{id}",
            get(read::get).put(update::update).delete(delete::delete),
        )
}

#[derive(OpenApi)]
#[openapi(
    paths(create::create, read::get, list::list, update::update, delete::delete),
    components(schemas(
        read::BlueprintOut,
        read::BlueprintDocumentOut,
        create::CreateBlueprintIn,
        update::UpdateBlueprintIn,
        list::BlueprintListOut
    ))
)]
pub struct ApiDoc;
