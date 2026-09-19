use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::{database::*, owner::*, status::*};

/// Every setup route is public: they exist so an install with no database (and so no
/// accounts to sign in with) can be brought up. `database` and `owner` close themselves
/// once the thing they set up exists.
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status", get(status))
        .route("/database", post(database))
        .route("/owner", post(owner))
}

#[derive(OpenApi)]
#[openapi(
    paths(status, database, owner),
    components(schemas(SetupStatus, DatabaseForm, OwnerForm)),
)]
pub struct ApiDoc;
