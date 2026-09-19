use std::sync::Arc;

use axum::{routing::get, Router};
use utoipa::OpenApi;

use crate::state::AppState;

use super::{create::*, delete::*, list::*};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/", get(list_users).post(create_user).delete(delete_user))
}

#[derive(OpenApi)]
#[openapi(
    paths(list_users, create_user, delete_user),
    components(schemas(CreateUserRequest)),
)]
pub struct ApiDoc;
