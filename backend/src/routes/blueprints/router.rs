use super::{
    catalog, clear_queue, create, delete, get as read, history, list, queue, reorder, run, start,
    stop, update, validate,
};
use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;
use utoipa::OpenApi;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/{id}/runs", get(history::history).post(start::start))
        .route("/{id}/runs/{run_id}", get(run::get_run))
        .route(
            "/queue",
            get(queue::queue)
                .put(reorder::reorder)
                .delete(clear_queue::clear_queue),
        )
        .route("/queue/stop", post(stop::stop))
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
        start::start,
        run::get_run,
        history::history,
        queue::queue,
        reorder::reorder,
        clear_queue::clear_queue,
        stop::stop,
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
