//! One row per server a user added. The live connection is not stored here.

use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};

use super::config::{ConnectionConfig, valid_server_name};

mod entities;
use entities::servers as entity;

pub fn migrations(prefix: &str) -> Vec<String> {
    vec![format!(
        "CREATE TABLE {prefix}_servers (
            id BIGSERIAL PRIMARY KEY,
            user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            config TEXT NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            last_error TEXT,
            UNIQUE (user_id, name)
        )"
    )]
}

#[derive(Clone, Debug)]
pub struct Server {
    pub name: String,
    pub config: ConnectionConfig,
    pub enabled: bool,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub enum StoreError {
    Db(sea_orm::DbErr),
    Invalid(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Db(e) => write!(f, "{e}"),
            StoreError::Invalid(e) => write!(f, "{e}"),
        }
    }
}

impl From<sea_orm::DbErr> for StoreError {
    fn from(e: sea_orm::DbErr) -> Self {
        StoreError::Db(e)
    }
}

#[derive(Clone)]
pub struct ServerStore {
    db: DatabaseConnection,
}

impl ServerStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn list(&self, user_id: i64) -> Result<Vec<Server>, StoreError> {
        let rows = entity::Entity::find()
            .filter(entity::Column::UserId.eq(user_id))
            .order_by_asc(entity::Column::Name)
            .all(&self.db)
            .await?;
        rows.into_iter().map(Self::server).collect()
    }

    /// Every server that should be connected after a restart.
    pub async fn list_enabled(&self) -> Result<Vec<(i64, Server)>, StoreError> {
        let rows = entity::Entity::find().filter(entity::Column::Enabled.eq(true)).all(&self.db).await?;
        rows.into_iter().map(|row| Ok((row.user_id, Self::server(row)?))).collect()
    }

    pub async fn upsert(&self, user_id: i64, name: &str, config: &ConnectionConfig, enabled: bool) -> Result<(), StoreError> {
        Self::check_name(name)?;
        config.validate().map_err(StoreError::Invalid)?;
        let config = serde_json::to_string(config).map_err(|e| StoreError::Invalid(e.to_string()))?;
        if let Some(row) = self.row(user_id, name).await? {
            let mut active: entity::ActiveModel = row.into();
            active.config = Set(config);
            active.enabled = Set(enabled);
            active.last_error = Set(None);
            active.update(&self.db).await?;
        } else {
            entity::ActiveModel {
                id: sea_orm::ActiveValue::NotSet,
                user_id: Set(user_id),
                name: Set(name.to_string()),
                config: Set(config),
                enabled: Set(enabled),
                last_error: Set(None),
            }
            .insert(&self.db)
            .await?;
        }
        Ok(())
    }

    pub async fn set_enabled(&self, user_id: i64, name: &str, enabled: bool) -> Result<(), StoreError> {
        let Some(row) = self.row(user_id, name).await? else {
            return Err(StoreError::Invalid(format!("no server named '{name}'")));
        };
        let mut active: entity::ActiveModel = row.into();
        active.enabled = Set(enabled);
        active.update(&self.db).await?;
        Ok(())
    }

    pub async fn set_error(&self, user_id: i64, name: &str, error: Option<String>) -> Result<(), StoreError> {
        let Some(row) = self.row(user_id, name).await? else { return Ok(()) };
        let mut active: entity::ActiveModel = row.into();
        active.last_error = Set(error);
        active.update(&self.db).await?;
        Ok(())
    }

    pub async fn remove(&self, user_id: i64, name: &str) -> Result<(), StoreError> {
        entity::Entity::delete_many()
            .filter(entity::Column::UserId.eq(user_id))
            .filter(entity::Column::Name.eq(name))
            .exec(&self.db)
            .await?;
        Ok(())
    }

    fn check_name(name: &str) -> Result<(), StoreError> {
        if valid_server_name(name) {
            Ok(())
        } else {
            Err(StoreError::Invalid(format!(
                "'{name}' is not a server name: lowercase letters, digits and underscores, at most 32 characters, and not 'mcp'"
            )))
        }
    }

    async fn row(&self, user_id: i64, name: &str) -> Result<Option<entity::Model>, StoreError> {
        Ok(entity::Entity::find()
            .filter(entity::Column::UserId.eq(user_id))
            .filter(entity::Column::Name.eq(name))
            .one(&self.db)
            .await?)
    }

    fn server(row: entity::Model) -> Result<Server, StoreError> {
        let config = serde_json::from_str(&row.config).map_err(|e| StoreError::Invalid(e.to_string()))?;
        Ok(Server { name: row.name, config, enabled: row.enabled, last_error: row.last_error })
    }
}
