use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    routes::auth::OwnerUser,
    services::{error::ErrorService, tls::CertInfo},
    state::AppState,
};

#[derive(Deserialize, ToSchema)]
pub(crate) struct CertificateRequest {
    /// The certificate in PEM, the server's first and any chain after it
    certificate: String,
    /// Its private key in PEM, without a password
    private_key: String,
}

/// Owner-only. Replaces the frontend container's HTTPS certificate with this one. It is checked first: PEM, the key
/// belongs to it, not expired. The container notices within a few seconds and serves it from then on.
#[utoipa::path(
    post,
    path = "/api/tls/certificate",
    tag = "tls",
    request_body = CertificateRequest,
    responses(
        (status = 200, description = "Stored", body = CertInfo),
        (status = 400, description = "Not a usable certificate and key", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn set_certificate(State(state): State<Arc<AppState>>, _owner: OwnerUser, Json(body): Json<CertificateRequest>) -> Result<Json<CertInfo>, ErrorService> {
    Ok(Json(state.tls.set_custom(&body.certificate, &body.private_key)?))
}
