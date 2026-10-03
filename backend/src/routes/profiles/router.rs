use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::create::*;
use super::delete::*;
use super::dto::*;
use super::get::*;
use super::update::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_profiles).post(create_profile).delete(delete_profile))
        .route("/update", post(update_profile))
}

#[derive(OpenApi)]
#[openapi(
    paths(get_profiles, create_profile, update_profile, delete_profile),
    components(schemas(LaunchProfileOut, LaunchProfileIn, ProfilesOut, CreateProfileRequest, UpdateProfileRequest)),
)]
pub struct ApiDoc;
