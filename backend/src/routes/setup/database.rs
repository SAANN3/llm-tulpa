use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::config::DbConfig;
use crate::services::error::ErrorService;
use crate::state::AppState;

#[derive(Deserialize, ToSchema)]
pub(crate) struct DatabaseForm {
    host: String,
    port: u16,
    name: String,
    user: String,
    password: String,
}

/// Public, first-run only: tests the given Postgres connection, and on success writes
/// `settings.json`, runs the migration, and swaps the live services in. A bad connection
/// returns 400 (without echoing connection details back) and writes nothing. Once a
/// database has been configured this is closed (403) — changing it means editing
/// `settings.json`, since an unauthenticated route must not be able to repoint the app.
#[utoipa::path(
    post,
    path = "/api/setup/database",
    tag = "setup",
    request_body = DatabaseForm,
    responses(
        (status = 204, description = "Connected, migrated and saved"),
        (status = 400, description = "Could not connect with those details", body = crate::services::error::ErrorBody),
        (status = 403, description = "A database is already configured", body = crate::services::error::ErrorBody),
        (status = 500, description = "Migration or saving the config failed", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn database(
    State(state): State<Arc<AppState>>,
    Json(body): Json<DatabaseForm>,
) -> Result<StatusCode, ErrorService> {
    state
        .configure(DbConfig {
            host: body.host,
            port: body.port,
            name: body.name,
            user: body.user,
            password: body.password,
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
