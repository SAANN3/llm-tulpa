use super::{catalog, create, delete, get as read, list, update, validate};
use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;
use utoipa::OpenApi;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/catalog", get(catalog::catalog))
        .route("/validate", post(validate::validate))
        .route("/", get(list::list).post(create::create))
        .route(
            "/{id}",
            get(read::get).put(update::update).delete(delete::delete),
        )
}

#[derive(OpenApi)]
#[openapi(
    paths(
        catalog::catalog,
        validate::validate,
        create::create,
        read::get,
        list::list,
        update::update,
        delete::delete
    ),
    components(schemas(
        read::BlueprintOut,
        read::BlueprintDocumentOut,
        create::CreateBlueprintIn,
        update::UpdateBlueprintIn,
        list::BlueprintListOut
    ))
)]
pub struct ApiDoc;
