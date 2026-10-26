mod entities;

use std::collections::HashMap;

use axum::http::StatusCode;
use entities::{active_model, llm_models, llm_providers, model_thinking_default, plugin_model};
use sea_orm::{
    prelude::*, sea_query::{Expr, OnConflict}, ActiveValue::Set, DatabaseConnection, DbErr, QueryOrder,
};

use crate::services::error::ErrorService;

/// Owns the `llm_providers`/`llm_models` tables — which backends exist and which models
/// the app knows about. Chats and user settings reference a model by id; anything that
/// needs a model's name (to send to the provider) resolves it through here.
pub struct ModelStore {
    db: DatabaseConnection,
}

/// The model an owner chose for a plugin's chats, with the launch profile chosen for it (llama.cpp models only).
#[derive(Clone, Debug)]
pub struct PluginModelChoice {
    pub model: ModelRef,
    pub launch_profile_id: Option<i64>,
}

/// A model together with the provider it belongs to.
#[derive(Clone, Debug)]
pub struct ModelRef {
    pub id: i64,
    pub provider: String,
    pub name: String,
    pub display_name: Option<String>,
}

impl ModelStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    fn to_ref(model: llm_models::Model, provider: llm_providers::Model) -> ModelRef {
        ModelRef { id: model.id, provider: provider.name, name: model.name, display_name: model.display_name }
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

    /// A registered model by provider and name, or `None` when nobody has picked or pulled it.
    pub async fn find(&self, provider: &str, name: &str) -> Result<Option<ModelRef>, ModelStoreErrors> {
        Ok(llm_models::Entity::find()
            .find_also_related(llm_providers::Entity)
            .filter(llm_providers::Column::Name.eq(provider))
            .filter(llm_models::Column::Name.eq(name))
            .one(&self.db)
            .await?
            .and_then(|(model, provider)| provider.map(|p| Self::to_ref(model, p))))
    }

    /// The models registered under `provider`, oldest first.
    pub async fn list(&self, provider: &str) -> Result<Vec<ModelRef>, ModelStoreErrors> {
        Ok(llm_models::Entity::find()
            .find_also_related(llm_providers::Entity)
            .filter(llm_providers::Column::Name.eq(provider))
            .order_by_asc(llm_models::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .filter_map(|(model, provider)| provider.map(|p| Self::to_ref(model, p)))
            .collect())
    }

    /// Whether a model other than `excluding` is registered, under any provider.
    pub async fn others_exist(&self, excluding: i64) -> Result<bool, ModelStoreErrors> {
        Ok(llm_models::Entity::find().filter(llm_models::Column::Id.ne(excluding)).one(&self.db).await?.is_some())
    }

    /// The model a user's new chats would start on if `excluding` did not exist: their chosen default
    /// unless that is `excluding`, else the oldest other model. `None` when there is no other model.
    pub async fn fallback_for_user(&self, user_id: i64, excluding: i64) -> Result<Option<ModelRef>, ModelStoreErrors> {
        let chosen = active_model::Entity::find_by_id(user_id).one(&self.db).await?.and_then(|s| s.active_model_id);
        if let Some(id) = chosen.filter(|id| *id != excluding) {
            if let Some(model) = self.get_many(&[id]).await?.remove(&id) {
                return Ok(Some(model));
            }
        }
        Ok(llm_models::Entity::find()
            .find_also_related(llm_providers::Entity)
            .filter(llm_models::Column::Id.ne(excluding))
            .order_by_asc(llm_models::Column::Id)
            .one(&self.db)
            .await?
            .and_then(|(model, provider)| provider.map(|p| Self::to_ref(model, p))))
    }

    /// Deletes a model. Its launch profiles and every user's choices of it go with it, and a user who
    /// had it as their default has none until they pick another; a chat still bound to it makes the
    /// database refuse, so the chats have to be moved first.
    pub async fn remove(&self, id: i64) -> Result<(), ModelStoreErrors> {
        llm_models::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    /// Sets (or, with `None`, clears) the name the UI shows for a model.
    pub async fn set_display_name(&self, id: i64, display_name: Option<String>) -> Result<(), ModelStoreErrors> {
        llm_models::Entity::update_many()
            .col_expr(llm_models::Column::DisplayName, Expr::value(display_name))
            .filter(llm_models::Column::Id.eq(id))
            .exec(&self.db)
            .await?;
        Ok(())
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

    /// The launch profile the user picked for their default model, when they did; the caller checks it
    /// still belongs to the model it is used with.
    pub async fn chosen_profile(&self, user_id: i64) -> Result<Option<i64>, ModelStoreErrors> {
        Ok(active_model::Entity::find_by_id(user_id).one(&self.db).await?.and_then(|settings| settings.active_profile_id))
    }

    /// The model chosen for one plugin's chats, when there is one and the model still exists.
    pub async fn plugin_model(&self, plugin_name: &str, plugin_subname: &str) -> Result<Option<PluginModelChoice>, ModelStoreErrors> {
        Ok(self.plugin_models().await?.remove(&(plugin_name.to_string(), plugin_subname.to_string())))
    }

    /// Every plugin's chosen model, keyed by (name, subname).
    pub async fn plugin_models(&self) -> Result<HashMap<(String, String), PluginModelChoice>, ModelStoreErrors> {
        let rows = plugin_model::Entity::find().all(&self.db).await?;
        let mut models = self.get_many(&rows.iter().map(|row| row.model_id).collect::<Vec<_>>()).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let model = models.remove(&row.model_id)?;
                Some(((row.plugin_name, row.plugin_subname), PluginModelChoice { model, launch_profile_id: row.launch_profile_id }))
            })
            .collect())
    }

    /// Chooses the model for a plugin's chats, replacing an earlier choice. The caller has checked that the profile belongs
    /// to the model.
    pub async fn set_plugin_model(&self, plugin_name: &str, plugin_subname: &str, model_id: i64, launch_profile_id: Option<i64>) -> Result<(), ModelStoreErrors> {
        plugin_model::Entity::insert(plugin_model::ActiveModel {
            plugin_name: Set(plugin_name.to_string()),
            plugin_subname: Set(plugin_subname.to_string()),
            model_id: Set(model_id),
            launch_profile_id: Set(launch_profile_id),
        })
        .on_conflict(
            OnConflict::columns([plugin_model::Column::PluginName, plugin_model::Column::PluginSubname])
                .update_columns([plugin_model::Column::ModelId, plugin_model::Column::LaunchProfileId])
                .to_owned(),
        )
        .exec(&self.db)
        .await?;
        Ok(())
    }

    /// Takes the choice away: the plugin's chats are left on the model they have.
    pub async fn clear_plugin_model(&self, plugin_name: &str, plugin_subname: &str) -> Result<(), ModelStoreErrors> {
        plugin_model::Entity::delete_by_id((plugin_name.to_string(), plugin_subname.to_string())).exec(&self.db).await?;
        Ok(())
    }

    /// The thinking level each of the user's models starts on, as (model, level) in no particular order. A model that was
    /// removed has no row left (the rows go with it).
    pub async fn thinking_defaults(&self, user_id: i64) -> Result<Vec<(ModelRef, String)>, ModelStoreErrors> {
        let rows = model_thinking_default::Entity::find().filter(model_thinking_default::Column::UserId.eq(user_id)).all(&self.db).await?;
        let mut models = self.get_many(&rows.iter().map(|row| row.model_id).collect::<Vec<_>>()).await?;
        Ok(rows.into_iter().filter_map(|row| Some((models.remove(&row.model_id)?, row.level))).collect())
    }

    /// Chooses the level a model starts on for the user, replacing an earlier one.
    pub async fn set_thinking_default(&self, user_id: i64, model_id: i64, level: &str) -> Result<(), ModelStoreErrors> {
        model_thinking_default::Entity::insert(model_thinking_default::ActiveModel {
            user_id: Set(user_id),
            model_id: Set(model_id),
            level: Set(level.to_string()),
        })
        .on_conflict(
            OnConflict::columns([model_thinking_default::Column::UserId, model_thinking_default::Column::ModelId])
                .update_column(model_thinking_default::Column::Level)
                .to_owned(),
        )
        .exec(&self.db)
        .await?;
        Ok(())
    }

    /// Takes the choice away: the model starts on its first level again.
    pub async fn clear_thinking_default(&self, user_id: i64, model_id: i64) -> Result<(), ModelStoreErrors> {
        model_thinking_default::Entity::delete_by_id((user_id, model_id)).exec(&self.db).await?;
        Ok(())
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
