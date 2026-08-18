use std::sync::Arc;

use axum::{routing::get, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::{delete::*, get::*, set::*};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(get_settings).post(set_settings).delete(delete_settings))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_settings, set_settings, delete_settings),
    components(schemas(crate::services::settings_store::Settings)),
)]
pub struct ApiDoc;
