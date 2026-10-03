use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    routes::auth::AuthUser,
    services::{
        error::ErrorService,
        llm::{LocalModel, LocalModelDetails},
    },
    state::AppState,
};

#[derive(Deserialize, IntoParams)]
pub(crate) struct ModelsQuery {
    /// `ollama` or `llama-cpp`; the provider of the caller's own default model when left out
    provider: Option<String>,
}

/// The models a provider has to offer right now — what a chat can switch to. For `ollama`, what is
/// installed in the connected Ollama (asked live, never cached). For `llama-cpp`, the model files
/// registered to run on the backend's own llama.cpp, with their size and quantization. Without
/// `provider` it answers for the provider of the caller's default model.
#[utoipa::path(
    get,
    path = "/api/llm/models",
    tag = "llm",
    params(ModelsQuery),
    responses(
        (status = 200, description = "The provider's models", body = [LocalModel]),
        (status = 409, description = "No such provider is configured", body = crate::services::error::ErrorBody),
        (status = 500, description = "Failed to reach or decode Ollama's response", body = crate::services::error::ErrorBody),
        (status = 502, description = "Ollama returned a non-success status", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn models(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ModelsQuery>,
) -> Result<Json<Vec<LocalModel>>, ErrorService> {
    let services = state.services().await?;
    let provider = match query.provider {
        Some(provider) => provider,
        None => services.settings_store.settings(auth.id).await?.llm_provider,
    };

    if provider == "llama-cpp" {
        let registered: Vec<String> = services.model_store.list(&provider).await?.into_iter().map(|m| m.name).collect();
        let files = state.library.local_files().await.files;
        let models = registered
            .into_iter()
            .map(|name| {
                let file = files.iter().find(|f| f.path == name);
                LocalModel {
                    size: file.map(|f| f.size_bytes),
                    details: Some(LocalModelDetails {
                        family: None,
                        parameter_size: None,
                        quantization_level: file.and_then(|f| f.quantization.clone()),
                    }),
                    name,
                }
            })
            .collect();
        return Ok(Json(models));
    }

    Ok(Json(state.providers.get(&provider)?.list_local_models().await?))
}
