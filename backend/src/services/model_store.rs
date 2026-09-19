use std::collections::HashMap;

use axum::http::StatusCode;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, QueryResult, Statement};

use crate::services::error::ErrorService;

const SELECT_MODEL: &str = "SELECT m.id AS id, p.name AS provider, m.name AS name
    FROM llm_models m JOIN llm_providers p ON p.id = m.provider_id";

/// Owns the `llm_providers`/`llm_models` tables — which backends exist and which models
/// the app knows about. Chats and user settings reference a model by id; anything that
/// needs a model's name (to send to the provider) resolves it through here. Plain raw SQL
/// rather than SeaORM entities since it's a small join-heavy lookup shared by the chat and
/// settings stores.
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

    fn to_ref(row: &QueryResult) -> Result<ModelRef, DbErr> {
        Ok(ModelRef {
            id: row.try_get("", "id")?,
            provider: row.try_get("", "provider")?,
            name: row.try_get("", "name")?,
        })
    }

    /// The names of every provider the app supports.
    pub async fn providers(&self) -> Result<Vec<String>, ModelStoreErrors> {
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                "SELECT name FROM llm_providers ORDER BY id",
            ))
            .await?;
        Ok(rows.iter().map(|r| r.try_get("", "name")).collect::<Result<_, _>>()?)
    }

    /// Registers `name` under `provider` if it isn't known yet, and returns it either way.
    /// Errs `UnknownProvider` for a provider the app doesn't support.
    pub async fn ensure(&self, provider: &str, name: &str) -> Result<ModelRef, ModelStoreErrors> {
        let provider_id: i64 = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT id FROM llm_providers WHERE name = $1",
                [provider.into()],
            ))
            .await?
            .map(|r| r.try_get("", "id"))
            .transpose()?
            .ok_or_else(|| ModelStoreErrors::UnknownProvider(provider.to_string()))?;

        let id: i64 = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO llm_models (provider_id, name) VALUES ($1, $2)
                 ON CONFLICT (provider_id, name) DO UPDATE SET name = EXCLUDED.name
                 RETURNING id",
                [provider_id.into(), name.into()],
            ))
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("llm_models insert returned no row".to_string()))?
            .try_get("", "id")?;

        Ok(ModelRef { id, provider: provider.to_string(), name: name.to_string() })
    }

    /// The given models by id, in one query. Ids that don't exist are simply absent.
    pub async fn get_many(&self, ids: &[i64]) -> Result<HashMap<i64, ModelRef>, ModelStoreErrors> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        // Ids are i64s we formatted ourselves, so inlining them can't inject anything.
        let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("{SELECT_MODEL} WHERE m.id IN ({list})"),
            ))
            .await?;

        let mut out = HashMap::with_capacity(rows.len());
        for row in &rows {
            let model = Self::to_ref(row)?;
            out.insert(model.id, model);
        }
        Ok(out)
    }

    /// The model a user's new chats and one-shot prompts use: their chosen active model,
    /// else the oldest registered model, else `None` (nothing has been picked yet).
    pub async fn default_for_user(&self, user_id: i64) -> Result<Option<ModelRef>, ModelStoreErrors> {
        let active = self
            .db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                format!(
                    "{SELECT_MODEL} JOIN user_settings s ON s.active_model_id = m.id WHERE s.user_id = $1"
                ),
                [user_id.into()],
            ))
            .await?;
        if let Some(row) = active {
            return Ok(Some(Self::to_ref(&row)?));
        }

        let first = self
            .db
            .query_one_raw(Statement::from_string(
                DbBackend::Postgres,
                format!("{SELECT_MODEL} ORDER BY m.id LIMIT 1"),
            ))
            .await?;
        Ok(first.map(|row| Self::to_ref(&row)).transpose()?)
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
