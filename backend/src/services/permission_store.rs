mod entities;
mod migrate;

use axum::http::StatusCode;
use entities::tool_permissions;
use migrate::migrate;
use sea_orm::{prelude::*, ActiveValue::Set, Database, DatabaseConnection, DbBackend, Statement};

use crate::services::error::ErrorService;

/// Owns per-chat tool-permission grants — which tools a chat has been allowed to use,
/// and under what scope (opaque JSON a tool defined; see `tools::base::ScopeGrant`).
/// Single responsibility, kept separate from both `ChatStore` (chat/message content,
/// not permissions) and `SettingsStore` (one global row, not per-chat/per-tool data).
/// Same isolated-SeaORM-entity shape as the other stores — `tool_permissions` is
/// private to this module, callers only ever see `serde_json::Value` scopes.
pub struct PermissionStore {
    db: DatabaseConnection,
}

impl PermissionStore {
    /// Same create-database-if-missing-then-migrate bootstrap as `ChatStore::new`. Note
    /// `tool_permissions` has a foreign key on `chats (id)` — this must be constructed
    /// after `ChatStore` (so the `chats` table already exists) for a fresh database's
    /// migration to succeed.
    pub async fn new(base_url: &str, db_name: &str) -> Self {
        let admin_url = format!("{base_url}/postgres");

        let admin_db = Database::connect(&admin_url).await.unwrap_or_else(|e| {
            panic!("failed to connect to postgres to check/create database '{db_name}': {e}")
        });

        let exists = admin_db
            .query_one_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "SELECT 1 FROM pg_database WHERE datname = $1",
                [db_name.into()],
            ))
            .await
            .unwrap_or_else(|e| panic!("failed to check whether database '{db_name}' exists: {e}"))
            .is_some();

        if !exists {
            admin_db
                .execute_unprepared(&format!("CREATE DATABASE \"{db_name}\""))
                .await
                .unwrap_or_else(|e| panic!("failed to create database '{db_name}': {e}"));
        }

        admin_db
            .close()
            .await
            .unwrap_or_else(|e| panic!("failed to close bootstrap connection: {e}"));

        let target_url = format!("{base_url}/{db_name}");

        let db = Database::connect(&target_url)
            .await
            .unwrap_or_else(|e| panic!("failed to connect to database '{db_name}': {e}"));

        migrate(&db)
            .await
            .unwrap_or_else(|e| panic!("failed to run migrations: {e}"));

        Self { db }
    }

    /// Whether a grant exists for this chat/tool pair, without fetching the scope
    /// itself — for callers that only need a yes/no.
    pub async fn has_scope(&self, chat_id: i64, tool_name: &str) -> Result<bool, PermissionStoreErrors> {
        let exists = tool_permissions::Entity::find()
            .filter(tool_permissions::Column::ChatId.eq(chat_id))
            .filter(tool_permissions::Column::ToolName.eq(tool_name))
            .one(&self.db)
            .await?
            .is_some();

        Ok(exists)
    }

    /// The granted scope for this chat/tool pair. Errs with `NotFound` if nothing's
    /// been granted yet — there's no default scope that would ever be more correct
    /// than "not granted".
    pub async fn get_scope(&self, chat_id: i64, tool_name: &str) -> Result<serde_json::Value, PermissionStoreErrors> {
        let row = tool_permissions::Entity::find()
            .filter(tool_permissions::Column::ChatId.eq(chat_id))
            .filter(tool_permissions::Column::ToolName.eq(tool_name))
            .one(&self.db)
            .await?
            .ok_or(PermissionStoreErrors::NotFound)?;

        Ok(row.scope)
    }

    /// Upserts the scope granted to a tool within a chat — a fresh grant if none
    /// existed yet, otherwise widening/replacing the existing one. Callers don't need
    /// to know or care which case applies; `create_scope` below does.
    pub async fn update_scope(
        &self,
        chat_id: i64,
        tool_name: &str,
        scope: serde_json::Value,
    ) -> Result<(), PermissionStoreErrors> {
        self.create_scope(chat_id, tool_name, scope).await
    }

    /// Inserts a grant, or replaces the existing one for this chat/tool pair —
    /// `ON CONFLICT` on the `(chat_id, tool_name)` unique constraint from `migrate`.
    /// Private: `update_scope` is the public entry point, since from a caller's
    /// perspective there's no meaningful difference between "grant this for the first
    /// time" and "grant this again" — both just mean "this scope is now permitted".
    async fn create_scope(
        &self,
        chat_id: i64,
        tool_name: &str,
        scope: serde_json::Value,
    ) -> Result<(), PermissionStoreErrors> {
        let existing = tool_permissions::Entity::find()
            .filter(tool_permissions::Column::ChatId.eq(chat_id))
            .filter(tool_permissions::Column::ToolName.eq(tool_name))
            .one(&self.db)
            .await?;

        let model = tool_permissions::ActiveModel {
            id: existing.as_ref().map_or(sea_orm::ActiveValue::NotSet, |row| Set(row.id)),
            chat_id: Set(chat_id),
            tool_name: Set(tool_name.to_string()),
            scope: Set(scope),
        };

        if existing.is_some() {
            model.update(&self.db).await?;
        } else {
            model.insert(&self.db).await?;
        }

        Ok(())
    }

    /// Revokes a chat's grant for a tool. Not an error if there wasn't one.
    pub async fn delete(&self, chat_id: i64, tool_name: &str) -> Result<(), PermissionStoreErrors> {
        tool_permissions::Entity::delete_many()
            .filter(tool_permissions::Column::ChatId.eq(chat_id))
            .filter(tool_permissions::Column::ToolName.eq(tool_name))
            .exec(&self.db)
            .await?;

        Ok(())
    }
}

/// Mirrors `ChatStoreErrors`'s/`SettingsStoreErrors`'s shape — one generic
/// `QueryFailed` wrapping any SeaORM failure, plus `NotFound` since that maps to a
/// different HTTP status (404, not 500) and isn't a failure from the database's point
/// of view.
#[derive(Debug)]
pub enum PermissionStoreErrors {
    QueryFailed(DbErr),
    NotFound,
}

impl From<DbErr> for PermissionStoreErrors {
    fn from(err: DbErr) -> Self {
        PermissionStoreErrors::QueryFailed(err)
    }
}

impl From<PermissionStoreErrors> for ErrorService {
    fn from(err: PermissionStoreErrors) -> Self {
        match err {
            PermissionStoreErrors::QueryFailed(e) => {
                tracing::error!("permission store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            PermissionStoreErrors::NotFound => {
                ErrorService::new(StatusCode::NOT_FOUND, "no permission grant for this chat/tool")
            }
        }
    }
}
