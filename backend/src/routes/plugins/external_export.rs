use std::sync::Arc;

use axum::{extract::Query, extract::State, http::header, response::IntoResponse};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

#[derive(Deserialize, IntoParams)]
pub struct ExportQuery {
    pub plugin_name: String,
    pub plugin_subname: String,
}

/// An external plugin's folder as a `.tulpa` file, without what building or running it left behind (`target/`, virtual
/// environments, caches), to install somewhere else
#[utoipa::path(
    get,
    path = "/api/plugins/external/export",
    tag = "plugins",
    params(ExportQuery),
    responses(
        (status = 200, description = "The `.tulpa` file", content_type = "application/zip", body = Vec<u8>),
        (status = 404, description = "No such external plugin", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn export_external_plugin(State(state): State<Arc<AppState>>, _owner: OwnerUser, Query(query): Query<ExportQuery>) -> Result<impl IntoResponse, ErrorService> {
    let (file_name, bytes) = state.services().await?.external_plugins.export(&query.plugin_name, &query.plugin_subname).await?;
    Ok(([(header::CONTENT_TYPE, "application/zip".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{file_name}\""))], bytes))
}
