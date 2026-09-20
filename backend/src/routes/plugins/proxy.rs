use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{StatusCode, Uri},
    response::Response,
};

use crate::{routes::auth::OwnerUser, services::error::ErrorService, state::AppState};

/// Forwards `/api/plugins/<plugin_name>/<plugin_subname>/...` into that plugin's own router
/// (with the two leading segments stripped, as if it were mounted there). The router is looked
/// up in the registry on every request, so a plugin rebuilt by a settings change — or a whole
/// set of services swapped in after first-run setup — is picked up without touching the route
/// table. A disabled or unknown plugin 404s. Every static registry route (list, settings, ...)
/// is matched before this, so it only ever sees paths nothing else claimed.
pub async fn plugin_proxy(
    State(state): State<Arc<AppState>>,
    _owner: OwnerUser,
    mut req: Request,
) -> Result<Response, ErrorService> {
    let not_found = || ErrorService::new(StatusCode::NOT_FOUND, "no such plugin route");
    let services = state.services().await?;

    let path = req.uri().path().to_string();
    let mut segments = path.trim_start_matches('/').splitn(3, '/');
    let (Some(plugin_name), Some(plugin_subname)) = (segments.next(), segments.next()) else {
        return Err(not_found());
    };
    let rest = segments.next().unwrap_or("");

    let path_and_query = match req.uri().query() {
        Some(query) => format!("/{rest}?{query}"),
        None => format!("/{rest}"),
    };
    *req.uri_mut() = path_and_query.parse::<Uri>().map_err(|_| not_found())?;

    services.plugin_registry.dispatch(plugin_name, plugin_subname, req).await
}
