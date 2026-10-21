use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use utoipa::OpenApi;

use crate::state::AppState;

use super::regenerate::*;
use super::remove::*;
use super::set::*;
use super::status::*;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(tls_status))
        .route("/certificate", post(set_certificate).delete(remove_certificate))
        .route("/regenerate", post(regenerate_certificate))
}

#[derive(OpenApi)]
#[openapi(
    paths(tls_status, set_certificate, remove_certificate, regenerate_certificate),
    components(schemas(
        CertificateRequest,
        crate::services::tls::TlsStatus,
        crate::services::tls::CertInfo,
    )),
)]
pub struct ApiDoc;
