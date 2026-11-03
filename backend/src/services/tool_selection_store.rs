mod entities;

use std::collections::{HashMap, HashSet};
use std::fmt;

use axum::http::StatusCode;
use entities::tool_names;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, DbErr, QueryOrder, TransactionTrait};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::services::error::ErrorService;

/// Which tools a chat, or a user's default for new chats, sends. `blacklist` is every tool except the
/// listed names. `whitelist` is only the listed names, and an empty one sends none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ToolMode {
    Blacklist,
    Whitelist,
}

impl fmt::Display for ToolMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ToolMode::Blacklist => "blacklist",
            ToolMode::Whitelist => "whitelist",
        })
    }
}

impl TryFrom<&str> for ToolMode {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, ()> {
        match value {
            "blacklist" => Ok(ToolMode::Blacklist),
            "whitelist" => Ok(ToolMode::Whitelist),
            _ => Err(()),
        }
    }
}

impl ToolMode {
    /// Whether `name` is sent under this mode.
    pub fn allows(self, listed: &[String], name: &str) -> bool {
        let listed = listed.iter().any(|listed| listed == name);
        match self {
            ToolMode::Whitelist => listed,
            ToolMode::Blacklist => !listed,
        }
    }

    /// Whether the prompt is the one that talks about tools. An empty whitelist is a chat without them.
    pub fn prompt_has_tools(self, listed: &[String]) -> bool {
        match self {
            ToolMode::Whitelist => !listed.is_empty(),
            ToolMode::Blacklist => true,
        }
    }
}

/// Owns `tool_names`: the names a chat's or a user's `ToolMode` lists. The mode itself is a column on
/// `chats` and `user_settings`, which those stores own.
pub struct ToolSelectionStore {
    db: DatabaseConnection,
}

impl ToolSelectionStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// A listed name has the shape every tool name has (`domain.action`), and none is repeated.
    /// The shape is checked here rather than through the plugin layer: this store does not depend on it.
    pub fn check_names(names: &[String]) -> Result<(), ToolSelectionError> {
        let mut seen = HashSet::new();
        for name in names {
            if !Self::name_shape(name) {
                return Err(ToolSelectionError::Invalid(format!("'{name}' is not a tool name (it looks like domain.action)")));
            }
            if !seen.insert(name.as_str()) {
                return Err(ToolSelectionError::Invalid(format!("'{name}' is listed twice")));
            }
        }
        Ok(())
    }

    fn name_shape(name: &str) -> bool {
        name.split_once('.').is_some_and(|(domain, action)| Self::word(domain) && Self::word(action))
    }

    fn word(part: &str) -> bool {
        part.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    /// The names `mode` lists for this chat, and whether its prompt is the one that talks about tools.
    pub async fn listed(&self, chat_id: i64, mode: ToolMode) -> Result<(Vec<String>, bool), ToolSelectionError> {
        let names = self.names_for_chat(chat_id).await?;
        let has_tools = mode.prompt_has_tools(&names);
        Ok((names, has_tools))
    }

    pub async fn names_for_chat(&self, chat_id: i64) -> Result<Vec<String>, ToolSelectionError> {
        let mut grouped = self.names_for_chats(&[chat_id]).await?;
        Ok(grouped.remove(&chat_id).unwrap_or_default())
    }

    pub async fn names_for_user(&self, user_id: i64) -> Result<Vec<String>, ToolSelectionError> {
        self.names_where(tool_names::Column::UserId.eq(user_id)).await
    }

    /// Replaces the names listed for a chat.
    pub async fn replace_chat(&self, chat_id: i64, names: &[String]) -> Result<(), ToolSelectionError> {
        self.replace(tool_names::Column::ChatId.eq(chat_id), Some(chat_id), None, names).await
    }

    /// Replaces the names listed for a user.
    pub async fn replace_user(&self, user_id: i64, names: &[String]) -> Result<(), ToolSelectionError> {
        self.replace(tool_names::Column::UserId.eq(user_id), None, Some(user_id), names).await
    }

    /// Copies one chat's listed names onto another. The mode column is the chat store's to copy.
    pub async fn copy_chat(&self, from_chat: i64, to_chat: i64) -> Result<(), ToolSelectionError> {
        let names = self.names_for_chat(from_chat).await?;
        self.replace_chat(to_chat, &names).await
    }

    async fn names_for_chats(&self, ids: &[i64]) -> Result<HashMap<i64, Vec<String>>, ToolSelectionError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = tool_names::Entity::find()
            .filter(tool_names::Column::ChatId.is_in(ids.to_vec()))
            .order_by_asc(tool_names::Column::Name)
            .all(&self.db)
            .await?;
        let mut grouped: HashMap<i64, Vec<String>> = HashMap::new();
        for row in rows {
            if let Some(chat_id) = row.chat_id {
                grouped.entry(chat_id).or_default().push(row.name);
            }
        }
        Ok(grouped)
    }

    async fn names_where(&self, owner: impl Into<sea_orm::sea_query::SimpleExpr>) -> Result<Vec<String>, ToolSelectionError> {
        let rows = tool_names::Entity::find()
            .filter(owner.into())
            .order_by_asc(tool_names::Column::Name)
            .all(&self.db)
            .await?;
        Ok(rows.into_iter().map(|row| row.name).collect())
    }

    async fn replace(
        &self,
        owner: impl Into<sea_orm::sea_query::SimpleExpr>,
        chat_id: Option<i64>,
        user_id: Option<i64>,
        names: &[String],
    ) -> Result<(), ToolSelectionError> {
        let txn = self.db.begin().await?;
        tool_names::Entity::delete_many().filter(owner.into()).exec(&txn).await?;
        for name in names {
            tool_names::ActiveModel {
                chat_id: Set(chat_id),
                user_id: Set(user_id),
                name: Set(name.clone()),
                ..Default::default()
            }
            .insert(&txn)
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ToolSelectionError {
    QueryFailed(DbErr),
    Invalid(String),
}

impl From<DbErr> for ToolSelectionError {
    fn from(err: DbErr) -> Self {
        ToolSelectionError::QueryFailed(err)
    }
}

impl From<ToolSelectionError> for ErrorService {
    fn from(err: ToolSelectionError) -> Self {
        match err {
            ToolSelectionError::QueryFailed(e) => {
                tracing::error!("tool selection query failed: {e}");
                ErrorService::internal("database query failed")
            }
            ToolSelectionError::Invalid(why) => ErrorService::new(StatusCode::BAD_REQUEST, why),
        }
    }
}
