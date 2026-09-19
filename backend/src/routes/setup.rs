use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::config::{self, DbConfig};
use crate::routes::auth::AuthResponse;
use crate::services::bootstrap::bootstrap;
use crate::services::error::ErrorService;
use crate::services::user_store::ROLE_OWNER;
use crate::state::AppState;

#[derive(Serialize)]
pub struct SetupStatus {
    /// Whether a database is connected and the app is usable.
    pub configured: bool,
    /// Whether the owner account has been created.
    pub has_owner: bool,
}

/// Public: reports whether the backend has a *working* database connection and an owner —
/// the frontend uses this to send a fresh (or disconnected) install to the setup wizard.
/// The connection is actually pinged, so a database that was configured but has since gone
/// away reads as not configured rather than leaving every other request to fail.
pub async fn status(State(state): State<Arc<AppState>>) -> Json<SetupStatus> {
    let services = state.services.read().await.clone();

    let (configured, has_owner) = match services {
        Some(services) if services.ping().await => {
            (true, services.user_store.user_count().await.unwrap_or(0) > 0)
        }
        _ => (false, false),
    };

    Json(SetupStatus { configured, has_owner })
}

#[derive(Deserialize)]
pub struct DatabaseForm {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String,
}

/// Public: tests the given Postgres connection, and on success writes `settings.json`,
/// runs the migration, and swaps the live services in. A bad connection returns 400 and
/// writes nothing.
pub async fn database(
    State(state): State<Arc<AppState>>,
    Json(body): Json<DatabaseForm>,
) -> Result<StatusCode, ErrorService> {
    let db = DbConfig {
        host: body.host,
        port: body.port,
        name: body.name,
        user: body.user,
        password: body.password,
    };
    let name = db.name.clone();

    let services = bootstrap(
        &db.base_url(),
        &name,
        state.files_dir.clone(),
        state.ollama.clone(),
        state.tools.clone(),
        state.agent_history_len,
        state.ollama_context_length,
    )
    .await
    .map_err(|e| ErrorService::new(StatusCode::BAD_REQUEST, e))?;

    {
        let mut cfg = state.config.write().await;
        cfg.database = Some(db);
        config::save(&cfg).map_err(|e| ErrorService::internal(format!("could not save config: {e}")))?;
    }
    *state.services.write().await = Some(services);

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct OwnerForm {
    pub username: String,
    pub password: String,
}

/// Public: creates the owner account — only allowed while no users exist. Returns a JWT
/// so the wizard is authenticated for the remaining (per-user) steps.
pub async fn owner(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OwnerForm>,
) -> Result<Json<AuthResponse>, ErrorService> {
    let services = state.services().await?;
    if services.user_store.user_count().await? > 0 {
        return Err(ErrorService::new(StatusCode::CONFLICT, "an owner account already exists"));
    }

    let user = services.user_store.create_user(&body.username, &body.password, ROLE_OWNER).await?;
    let token = state.auth.issue(user.id, &user.username, &user.role)?;
    Ok(Json(AuthResponse { token, user }))
}
