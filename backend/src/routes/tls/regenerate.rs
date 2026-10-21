use std::sync::Arc;

use axum::{extract::State, http::{header, HeaderMap, StatusCode}};

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, tls::names_of_this_machine},
    state::AppState,
};

/// The name or address in a `Host` header, without its port
fn host_of(headers: &HeaderMap) -> Option<String> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    // `[::1]:5443` and `name:5443`
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next()?,
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    };
    Some(name.to_string())
}

/// Owner-only. Asks the frontend container for a new self-signed certificate, for this machine's addresses as they are now
/// and the one this request came to (what a browser has to find on the certificate, after the machine's address changed).
/// The container makes it within seconds and serves it unless an uploaded certificate is in use.
#[utoipa::path(
    post,
    path = "/api/tls/regenerate",
    tag = "tls",
    responses((status = 202, description = "Asked for")),
)]
pub async fn regenerate_certificate(State(state): State<Arc<AppState>>, _owner: OwnerUser, headers: HeaderMap) -> Result<StatusCode, ErrorService> {
    state.tls.regenerate(&names_of_this_machine(host_of(&headers).as_deref()))?;
    Ok(StatusCode::ACCEPTED)
}
