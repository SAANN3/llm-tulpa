use std::sync::Arc;

use axum::{routing::get, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::{get::*, set::*};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(get_settings).post(set_settings))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_settings, set_settings),
    components(schemas(
        crate::services::settings_store::Settings,
        crate::services::settings_store::SettingsUpdate,
    )),
)]
pub struct ApiDoc;
