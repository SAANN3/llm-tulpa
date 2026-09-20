mod entities;

use std::collections::HashMap;

use axum::http::StatusCode;
use entities::{active_model, llm_models, llm_providers};
use sea_orm::{
    prelude::*, sea_query::OnConflict, ActiveValue::Set, DatabaseConnection, DbErr, QueryOrder,
};

use crate::services::error::ErrorService;

/// Owns the `llm_providers`/`llm_models` tables — which backends exist and which models
/// the app knows about. Chats and user settings reference a model by id; anything that
/// needs a model's name (to send to the provider) resolves it through here.
pub struct ModelStore {
    db: DatabaseConnection,
}

/// A model together with the provider it belongs to.
#[derive(Clone, Debug)]
pub struct ModelRef {
    pub id: i64,
    pub provider: String,
    pub name: String,
}

impl ModelStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    fn to_ref(model: llm_models::Model, provider: llm_providers::Model) -> ModelRef {
        ModelRef { id: model.id, provider: provider.name, name: model.name }
    }

    /// The names of every provider the app supports.
    pub async fn providers(&self) -> Result<Vec<String>, ModelStoreErrors> {
        Ok(llm_providers::Entity::find()
            .order_by_asc(llm_providers::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|p| p.name)
            .collect())
    }

    /// Whether `name` is already registered under `provider` — i.e. some user has picked or
    /// pulled it before.
    pub async fn is_registered(&self, provider: &str, name: &str) -> Result<bool, ModelStoreErrors> {
        Ok(llm_models::Entity::find()
            .find_also_related(llm_providers::Entity)
            .filter(llm_providers::Column::Name.eq(provider))
            .filter(llm_models::Column::Name.eq(name))
            .one(&self.db)
            .await?
            .is_some())
    }

    /// Registers `name` under `provider` if it isn't known yet, and returns it either way.
    /// Errs `UnknownProvider` for a provider the app doesn't support.
    pub async fn ensure(&self, provider: &str, name: &str) -> Result<ModelRef, ModelStoreErrors> {
        let provider = llm_providers::Entity::find()
            .filter(llm_providers::Column::Name.eq(provider))
            .one(&self.db)
            .await?
            .ok_or_else(|| ModelStoreErrors::UnknownProvider(provider.to_string()))?;

        // The no-op update makes `RETURNING` yield the existing row on a conflict too, which
        // `DO NOTHING` wouldn't.
        let model = llm_models::Entity::insert(llm_models::ActiveModel {
            provider_id: Set(provider.id),
            name: Set(name.to_string()),
            ..Default::default()
        })
        .on_conflict(
            OnConflict::columns([llm_models::Column::ProviderId, llm_models::Column::Name])
                .update_column(llm_models::Column::Name)
                .to_owned(),
        )
        .exec_with_returning(&self.db)
        .await?;

        Ok(Self::to_ref(model, provider))
    }

    /// The given models by id, in one query. Ids that don't exist are simply absent.
    pub async fn get_many(&self, ids: &[i64]) -> Result<HashMap<i64, ModelRef>, ModelStoreErrors> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut out = HashMap::with_capacity(ids.len());
        for (model, provider) in llm_models::Entity::find()
            .filter(llm_models::Column::Id.is_in(ids.iter().copied()))
            .find_also_related(llm_providers::Entity)
            .all(&self.db)
            .await?
        {
            if let Some(provider) = provider {
                out.insert(model.id, Self::to_ref(model, provider));
            }
        }
        Ok(out)
    }

    /// The model a user's new chats and one-shot prompts use: their chosen active model,
    /// else the oldest registered model, else `None` (nothing has been picked yet).
    pub async fn default_for_user(&self, user_id: i64) -> Result<Option<ModelRef>, ModelStoreErrors> {
        let active_id = active_model::Entity::find_by_id(user_id)
            .one(&self.db)
            .await?
            .and_then(|settings| settings.active_model_id);

        if let Some(id) = active_id {
            if let Some(model) = self.get_many(&[id]).await?.remove(&id) {
                return Ok(Some(model));
            }
        }

        Ok(llm_models::Entity::find()
            .find_also_related(llm_providers::Entity)
            .order_by_asc(llm_models::Column::Id)
            .one(&self.db)
            .await?
            .and_then(|(model, provider)| provider.map(|p| Self::to_ref(model, p))))
    }

    /// `default_for_user`, erroring `NoModel` when nothing has been picked yet.
    pub async fn resolve_default(&self, user_id: i64) -> Result<ModelRef, ModelStoreErrors> {
        self.default_for_user(user_id).await?.ok_or(ModelStoreErrors::NoModel)
    }
}

#[derive(Debug)]
pub enum ModelStoreErrors {
    QueryFailed(DbErr),
    UnknownProvider(String),
    NoModel,
}

impl From<DbErr> for ModelStoreErrors {
    fn from(err: DbErr) -> Self {
        ModelStoreErrors::QueryFailed(err)
    }
}

impl From<ModelStoreErrors> for ErrorService {
    fn from(err: ModelStoreErrors) -> Self {
        match err {
            ModelStoreErrors::QueryFailed(e) => {
                tracing::error!("model store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            ModelStoreErrors::UnknownProvider(provider) => {
                ErrorService::new(StatusCode::BAD_REQUEST, format!("unknown LLM provider '{provider}'"))
            }
            ModelStoreErrors::NoModel => {
                ErrorService::new(StatusCode::CONFLICT, "no model is selected yet — pick one in Settings")
            }
        }
    }
}
