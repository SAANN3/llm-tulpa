use std::sync::Arc;

use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct SetupStatus {
    /// Whether a database is connected and the app is usable.
    configured: bool,
    /// Whether the owner account has been created.
    has_owner: bool,
}

/// Public: reports whether the backend has a *working* database connection and an owner —
/// the frontend uses this to send a fresh (or disconnected) install to the setup wizard.
/// The connection is actually pinged, so a database that went away after startup reads as
/// not configured rather than leaving every other request to fail. If a database is
/// configured but the services never came up (Postgres wasn't ready at boot), this is also
/// where they get another chance to.
#[utoipa::path(
    get,
    path = "/api/setup/status",
    tag = "setup",
    responses(
        (status = 200, description = "Whether the install is usable", body = SetupStatus),
    ),
)]
pub async fn status(State(state): State<Arc<AppState>>) -> Json<SetupStatus> {
    if let Err(e) = state.connect_configured().await {
        tracing::warn!("retrying the configured database connection failed: {e}");
    }

    let services = state.services.read().await.clone();
    let (configured, has_owner) = match services {
        Some(services) if services.ping().await => {
            (true, services.user_store.user_count().await.unwrap_or(0) > 0)
        }
        _ => (false, false),
    };

    Json(SetupStatus { configured, has_owner })
}
