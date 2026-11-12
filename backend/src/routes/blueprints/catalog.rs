use crate::{
    facade::blueprint::catalog::NodeDescriptor, routes::auth::AuthUser,
    services::error::ErrorService, state::AppState,
};
use axum::{Json, extract::State};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub(crate) struct CatalogOut {
    pub nodes: Vec<NodeDescriptor>,
}

#[utoipa::path(get, path = "/api/blueprints/catalog", tag = "blueprints",
    responses((status = 200, body = CatalogOut)))]
pub async fn catalog(
    State(state): State<Arc<AppState>>,
    _auth: AuthUser,
) -> Result<Json<CatalogOut>, ErrorService> {
    Ok(Json(CatalogOut {
        nodes: state.services().await?.blueprints.catalog(),
    }))
}
