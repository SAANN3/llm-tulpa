mod entities;

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::StatusCode;
use entities::{chats, message_files, message_images, messages, plugin_chats, tool_calls};
use sea_orm::{
    prelude::*, sea_query::Expr, ActiveValue::Set, DatabaseConnection, PaginatorTrait,
    QueryOrder, QuerySelect, TransactionError, TransactionTrait,
};
use serde::{Deserialize, Serialize};

use crate::services::error::ErrorService;
use crate::services::model_store::{ModelRef, ModelStore, ModelStoreErrors};
use crate::services::user_store::{UserStore, UserStoreErrors};

/// Owns chat/message persistence, scoped per user. SeaORM entities are private to this
/// module — callers only ever see the plain structs (`Chat`, `Message`, ...). Attached
/// images and files are normalized into `message_images`/`message_files` and reassembled
/// onto `Message` here, so nothing outside this store sees the split. Every chat carries
/// the model it's bound to, resolved through `ModelStore`.
pub struct ChatStore {
    db: DatabaseConnection,
    models: Arc<ModelStore>,
    /// Only for `owner_id`: plugin chats belong to the owner, and the `users` table is
    /// `UserStore`'s, not this store's.
    users: Arc<UserStore>,
}

impl ChatStore {
    /// Holds an already-connected, already-migrated connection (see `services::bootstrap`).
    pub fn new(db: DatabaseConnection, models: Arc<ModelStore>, users: Arc<UserStore>) -> Self {
        Self { db, models, users }
    }

    /// A chat by id — the one place that decides whether a chat is usable (exists and
    /// isn't soft-deleted). Does NOT check ownership; callers acting on behalf of a user
    /// use `owned_chat` instead.
    pub async fn chat(&self, chat_id: i64) -> Result<Chat, ChatStoreErrors> {
        let chat = chats::Entity::find_by_id(chat_id)
            .one(&self.db)
            .await?
            .filter(|chat| !chat.is_deleted)
            .ok_or(ChatStoreErrors::NotFound)?;

        self.to_chat(chat).await
    }

    /// Like `chat`, but 404s unless the chat belongs to `user_id` — the ownership gate
    /// handlers use before acting on a chat.
    pub async fn owned_chat(&self, user_id: i64, chat_id: i64) -> Result<Chat, ChatStoreErrors> {
        let chat = self.chat(chat_id).await?;
        if chat.user_id != user_id {
            return Err(ChatStoreErrors::NotFound);
        }
        Ok(chat)
    }

    /// A user's non-deleted, non-plugin chats, newest-active first, plus the total count.
    pub async fn chats(&self, user_id: i64, limit: u64, skip: u64) -> Result<(Vec<Chat>, u64), ChatStoreErrors> {
        // Plugin-owned chats are excluded by anti-joining `plugin_chats`.
        let plugin_chat_ids: Vec<i64> = plugin_chats::Entity::find()
            .select_only()
            .column(plugin_chats::Column::ChatId)
            .into_tuple()
            .all(&self.db)
            .await?;

        let query = chats::Entity::find()
            .filter(chats::Column::UserId.eq(user_id))
            .filter(chats::Column::IsDeleted.eq(false))
            .filter(chats::Column::Id.is_not_in(plugin_chat_ids));

        self.paginated_chats(query, limit, skip).await
    }

    /// Same shape as `chats`, scoped to one plugin instance's chats instead.
    pub async fn chats_by_plugin(
        &self,
        plugin_name: &str,
        plugin_subname: &str,
        limit: u64,
        skip: u64,
    ) -> Result<(Vec<Chat>, u64), ChatStoreErrors> {
        let chat_ids: Vec<i64> = plugin_chats::Entity::find()
            .filter(plugin_chats::Column::PluginName.eq(plugin_name))
            .filter(plugin_chats::Column::PluginSubname.eq(plugin_subname))
            .select_only()
            .column(plugin_chats::Column::ChatId)
            .into_tuple()
            .all(&self.db)
            .await?;

        let query = chats::Entity::find()
            .filter(chats::Column::IsDeleted.eq(false))
            .filter(chats::Column::Id.is_in(chat_ids));

        self.paginated_chats(query, limit, skip).await
    }

    async fn paginated_chats(
        &self,
        query: Select<chats::Entity>,
        limit: u64,
        skip: u64,
    ) -> Result<(Vec<Chat>, u64), ChatStoreErrors> {
        let total = query.clone().count(&self.db).await?;

        let rows = query
            .order_by_desc(chats::Column::UpdatedAt)
            .limit(limit)
            .offset(skip)
            .all(&self.db)
            .await?;

        Ok((self.to_chats(rows).await?, total))
    }

    /// Maps rows into `Chat`s, resolving every row's model in one batched lookup.
    async fn to_chats(&self, rows: Vec<chats::Model>) -> Result<Vec<Chat>, ChatStoreErrors> {
        let mut ids: Vec<i64> = rows.iter().map(|row| row.model_id).collect();
        ids.sort_unstable();
        ids.dedup();
        let models = self.models.get_many(&ids).await?;

        rows.into_iter()
            .map(|row| {
                let model = models
                    .get(&row.model_id)
                    .cloned()
                    .ok_or(ChatStoreErrors::NotFound)?;
                Ok(Self::build_chat(row, model))
            })
            .collect()
    }

    async fn to_chat(&self, row: chats::Model) -> Result<Chat, ChatStoreErrors> {
        Ok(self.to_chats(vec![row]).await?.remove(0))
    }

    fn build_chat(row: chats::Model, model: ModelRef) -> Chat {
        Chat {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            model_id: model.id,
            provider: model.provider,
            model: model.name,
            created_at: row.created_at,
            updated_at: row.updated_at,
            summary: row.summary,
            summary_up_to_message_id: row.summary_up_to_message_id,
            key_facts: row.key_facts.and_then(|v| serde_json::from_value(v).ok()),
        }
    }

    /// Messages for a chat, newest first (`skip` counts from the newest end). Each message
    /// includes its tool calls, images and files. Also returns the total message count.
    pub async fn messages(
        &self,
        chat_id: i64,
        limit: u64,
        skip: u64,
    ) -> Result<(Vec<Message>, u64), ChatStoreErrors> {
        self.chat(chat_id).await?;

        let query = messages::Entity::find().filter(messages::Column::ChatId.eq(chat_id));
        let total = query.clone().count(&self.db).await?;

        let rows = query
            .order_by_desc(messages::Column::CreatedAt)
            .limit(limit)
            .offset(skip)
            .all(&self.db)
            .await?;

        Ok((self.hydrate_messages(rows).await?, total))
    }

    /// Every message after `after_message_id` (exclusive), newest first, up to `limit`.
    pub async fn messages_after(
        &self,
        chat_id: i64,
        after_message_id: i64,
        limit: u64,
    ) -> Result<Vec<Message>, ChatStoreErrors> {
        self.chat(chat_id).await?;

        let rows = messages::Entity::find()
            .filter(messages::Column::ChatId.eq(chat_id))
            .filter(messages::Column::Id.gt(after_message_id))
            .order_by_desc(messages::Column::CreatedAt)
            .limit(limit)
            .all(&self.db)
            .await?;

        self.hydrate_messages(rows).await
    }

    /// Attaches each row's tool calls, images and files (one batched query each) and maps
    /// into the plain `Message` shape.
    async fn hydrate_messages(&self, rows: Vec<messages::Model>) -> Result<Vec<Message>, ChatStoreErrors> {
        let ids: Vec<i64> = rows.iter().map(|m| m.id).collect();

        let mut tool_calls = self.tool_calls_by_messages(ids.clone()).await?;
        let mut images = self.images_by_messages(ids.clone()).await?;
        let mut file_ids = self.files_by_messages(ids).await?;

        Ok(rows
            .into_iter()
            .map(|message| Message {
                id: message.id,
                chat_id: message.chat_id,
                role: message.role,
                content: message.content,
                tool_name: message.tool_name,
                created_at: message.created_at,
                thinking: message.thinking,
                thought_duration_ms: message.thought_duration_ms,
                tool_success: message.tool_success,
                tool_denied: message.tool_denied,
                tool_calls: tool_calls.remove(&message.id).unwrap_or_default(),
                images: images.remove(&message.id).unwrap_or_default(),
                file_ids: file_ids.remove(&message.id).unwrap_or_default(),
            })
            .collect())
    }

    /// Every tool call for a batch of messages in one query, grouped by `message_id` and
    /// ordered by `id` within each group — insertion order is the order the model requested
    /// them in, which callers need to resolve "which of these calls is next" without a separate
    /// ordering column. A single `LEFT JOIN` (or `find_with_related`) doesn't compose with the
    /// `LIMIT`/`OFFSET` `messages` applies to the message side: the join would cap joined
    /// (message, tool_call) *pairs*, not distinct messages. Two queries plus grouping in Rust
    /// sidesteps that at negligible cost.
    async fn tool_calls_by_messages(
        &self,
        message_ids: Vec<i64>,
    ) -> Result<HashMap<i64, Vec<ToolCallOut>>, ChatStoreErrors> {
        let calls = tool_calls::Entity::find()
            .filter(tool_calls::Column::MessageId.is_in(message_ids))
            .order_by_asc(tool_calls::Column::Position)
            .order_by_asc(tool_calls::Column::Id)
            .all(&self.db)
            .await?;

        let mut grouped: HashMap<i64, Vec<ToolCallOut>> = HashMap::new();
        for call in calls {
            grouped.entry(call.message_id).or_default().push(ToolCallOut {
                tool_name: call.tool_name,
                arguments: call.arguments,
            });
        }
        Ok(grouped)
    }

    async fn images_by_messages(&self, message_ids: Vec<i64>) -> Result<HashMap<i64, Vec<String>>, ChatStoreErrors> {
        let rows = message_images::Entity::find()
            .filter(message_images::Column::MessageId.is_in(message_ids))
            .order_by_asc(message_images::Column::Position)
            .order_by_asc(message_images::Column::Id)
            .all(&self.db)
            .await?;

        let mut grouped: HashMap<i64, Vec<String>> = HashMap::new();
        for row in rows {
            grouped.entry(row.message_id).or_default().push(row.data);
        }
        Ok(grouped)
    }

    async fn files_by_messages(&self, message_ids: Vec<i64>) -> Result<HashMap<i64, Vec<i64>>, ChatStoreErrors> {
        let rows = message_files::Entity::find()
            .filter(message_files::Column::MessageId.is_in(message_ids))
            .order_by_asc(message_files::Column::Position)
            .all(&self.db)
            .await?;

        let mut grouped: HashMap<i64, Vec<i64>> = HashMap::new();
        for row in rows {
            grouped.entry(row.message_id).or_default().push(row.file_id);
        }
        Ok(grouped)
    }

    /// Creates an ordinary (non-plugin) chat owned by `user_id`, bound to the user's
    /// default model (their active model, else the first registered one). Errs `Model(NoModel)`
    /// when no model exists at all — a chat can't be created without one.
    pub async fn create_chat(&self, user_id: i64, name: String) -> Result<Chat, ChatStoreErrors> {
        let model = self.models.resolve_default(user_id).await?;
        let row = chats::ActiveModel {
            user_id: Set(user_id),
            name: Set(name),
            model_id: Set(model.id),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;

        Ok(Self::build_chat(row, model))
    }

    /// Rebinds a chat to another model, which takes effect from its next turn. Ownership is
    /// the caller's responsibility.
    pub async fn set_model(&self, chat_id: i64, model_id: i64) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;
        chats::ActiveModel { id: Set(chat_id), model_id: Set(model_id), ..Default::default() }
            .update(&self.db)
            .await?;
        Ok(())
    }

    /// The owner user's id — plugin chats belong to the owner. Errs `NotFound` if no owner
    /// exists yet (setup hasn't run).
    async fn owner_id(&self) -> Result<i64, ChatStoreErrors> {
        self.users.owner_id().await?.ok_or(ChatStoreErrors::NotFound)
    }

    /// Creates a chat owned by one plugin instance (belonging to the owner user), mapped
    /// to that plugin's own external chat id.
    pub async fn create_plugin_chat(
        &self,
        name: String,
        plugin_name: String,
        plugin_subname: String,
        plugin_chat_id: String,
    ) -> Result<Chat, ChatStoreErrors> {
        let owner = self.owner_id().await?;
        let model = self.models.resolve_default(owner).await?;
        let chat = chats::ActiveModel {
            user_id: Set(owner),
            name: Set(name),
            model_id: Set(model.id),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;

        plugin_chats::ActiveModel {
            chat_id: Set(chat.id),
            plugin_name: Set(plugin_name),
            plugin_subname: Set(plugin_subname),
            plugin_chat_id: Set(plugin_chat_id),
        }
        .insert(&self.db)
        .await?;

        Ok(Self::build_chat(chat, model))
    }

    /// The chat mapped to one plugin instance's external chat id, or `None` if unmapped.
    pub async fn find_by_plugin_mapped_id(
        &self,
        plugin_name: &str,
        plugin_subname: &str,
        plugin_chat_id: &str,
    ) -> Result<Option<Chat>, ChatStoreErrors> {
        let Some(mapping) = plugin_chats::Entity::find()
            .filter(plugin_chats::Column::PluginName.eq(plugin_name))
            .filter(plugin_chats::Column::PluginSubname.eq(plugin_subname))
            .filter(plugin_chats::Column::PluginChatId.eq(plugin_chat_id))
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };

        let chat = chats::Entity::find_by_id(mapping.chat_id)
            .one(&self.db)
            .await?
            .filter(|c| !c.is_deleted);
        match chat {
            Some(row) => Ok(Some(self.to_chat(row).await?)),
            None => Ok(None),
        }
    }

    /// `find_by_plugin_mapped_id`, creating a new plugin chat the first time this external
    /// chat id is seen.
    pub async fn find_or_create_plugin_chat(
        &self,
        name: String,
        plugin_name: String,
        plugin_subname: String,
        plugin_chat_id: String,
    ) -> Result<Chat, ChatStoreErrors> {
        if let Some(chat) = self
            .find_by_plugin_mapped_id(&plugin_name, &plugin_subname, &plugin_chat_id)
            .await?
        {
            return Ok(chat);
        }
        self.create_plugin_chat(name, plugin_name, plugin_subname, plugin_chat_id).await
    }

    /// Persists an updated compaction summary for a chat — folds everything up to and
    /// including `up_to_message_id` into `summary`, so the next `ollama_history` build
    /// (in `Agent`) sends the summary plus only what's newer instead of the full
    /// history. Also persists updated `key_facts` (or clears them if empty). This is
    /// replay bookkeeping, not conversational content — nothing about the chat's actual
    /// message rows changes.
    pub async fn set_summary(
        &self,
        chat_id: i64,
        summary: String,
        key_facts: ChatFacts,
        up_to_message_id: i64,
    ) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;

        // Persist NULL when the struct is empty (goal is None and facts is empty) —
        // same convention as `summary`/`images`/`file_ids` above, so a pre-feature
        // chat (no facts at all) stays indistinguishable from one explicitly cleared.
        let key_facts_json = (!key_facts.is_empty()).then(|| serde_json::json!(key_facts));

        chats::ActiveModel {
            id: Set(chat_id),
            summary: Set(Some(summary)),
            summary_up_to_message_id: Set(Some(up_to_message_id)),
            key_facts: Set(key_facts_json),
            ..Default::default()
        }
        .update(&self.db)
        .await?;

        Ok(())
    }

    /// Renames the given chat.
    pub async fn rename_chat(&self, chat_id: i64, name: String) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;
        chats::ActiveModel { id: Set(chat_id), name: Set(name), ..Default::default() }
            .update(&self.db)
            .await?;
        Ok(())
    }

    /// Soft-deletes the chat: it's marked `is_deleted` rather than removed with its messages,
    /// so `chat`/`chats` just stop returning it.
    pub async fn delete_chat(&self, chat_id: i64) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;
        chats::ActiveModel { id: Set(chat_id), is_deleted: Set(true), ..Default::default() }
            .update(&self.db)
            .await?;
        Ok(())
    }

    /// Deletes every message of a chat (children cascade) and clears its summary, leaving the
    /// chat row itself intact — its id, name and, for a plugin chat, its external mapping — so
    /// the same conversation keeps working with a clean slate.
    ///
    /// Deliberately not built on `delete_chat`'s soft delete: a plugin chat's
    /// `(plugin_name, plugin_subname, plugin_chat_id)` is uniquely constrained across *all*
    /// rows regardless of `is_deleted`, so soft-deleting one would block that external chat
    /// from ever being linked again.
    pub async fn clear_messages(&self, chat_id: i64) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;

        self.db
            .transaction::<_, (), DbErr>(|txn| {
                Box::pin(async move {
                    messages::Entity::delete_many()
                        .filter(messages::Column::ChatId.eq(chat_id))
                        .exec(txn)
                        .await?;

                    chats::ActiveModel {
                        id: Set(chat_id),
                        summary: Set(None),
                        summary_up_to_message_id: Set(None),
                        key_facts: Set(None),
                        ..Default::default()
                    }
                    .update(txn)
                    .await?;

                    Ok(())
                })
            })
            .await
            .map_err(|err| match err {
                TransactionError::Connection(e) | TransactionError::Transaction(e) => ChatStoreErrors::from(e),
            })?;

        Ok(())
    }

    /// Inserts a message plus its tool calls, images and files, and bumps the parent
    /// chat's `updated_at` — all in one transaction.
    pub async fn new_message(&self, new_message: NewMessage) -> Result<Message, ChatStoreErrors> {
        let NewMessage {
            chat_id,
            role,
            content,
            tool_name,
            thinking,
            thought_duration_ms,
            tool_success,
            tool_denied,
            tool_calls,
            images,
            file_ids,
        } = new_message;

        let tool_calls_out: Vec<ToolCallOut> = tool_calls
            .iter()
            .map(|call| ToolCallOut { tool_name: call.tool_name.clone(), arguments: call.arguments.clone() })
            .collect();
        let images_ret = images.clone();
        let file_ids_ret = file_ids.clone();

        let message = self
            .db
            .transaction::<_, messages::Model, DbErr>(|txn| {
                Box::pin(async move {
                    let message = messages::ActiveModel {
                        chat_id: Set(chat_id),
                        role: Set(role),
                        content: Set(content),
                        tool_name: Set(tool_name),
                        thinking: Set(thinking),
                        thought_duration_ms: Set(thought_duration_ms),
                        tool_success: Set(tool_success),
                        tool_denied: Set(tool_denied),
                        ..Default::default()
                    }
                    .insert(txn)
                    .await?;

                    for (index, tool_call) in tool_calls.into_iter().enumerate() {
                        tool_calls::ActiveModel {
                            message_id: Set(message.id),
                            tool_name: Set(tool_call.tool_name),
                            arguments: Set(tool_call.arguments),
                            position: Set(index as i32),
                            ..Default::default()
                        }
                        .insert(txn)
                        .await?;
                    }

                    for (index, data) in images.into_iter().enumerate() {
                        message_images::ActiveModel {
                            message_id: Set(message.id),
                            position: Set(index as i32),
                            data: Set(data),
                            ..Default::default()
                        }
                        .insert(txn)
                        .await?;
                    }

                    for (index, file_id) in file_ids.into_iter().enumerate() {
                        message_files::ActiveModel {
                            message_id: Set(message.id),
                            file_id: Set(file_id),
                            position: Set(index as i32),
                        }
                        .insert(txn)
                        .await?;
                    }

                    chats::Entity::update_many()
                        .col_expr(chats::Column::UpdatedAt, Expr::cust("now()"))
                        .filter(chats::Column::Id.eq(chat_id))
                        .exec(txn)
                        .await?;

                    Ok(message)
                })
            })
            .await
            .map_err(|err| match err {
                TransactionError::Connection(e) | TransactionError::Transaction(e) => ChatStoreErrors::from(e),
            })?;

        Ok(Message {
            id: message.id,
            chat_id: message.chat_id,
            role: message.role,
            content: message.content,
            tool_name: message.tool_name,
            created_at: message.created_at,
            thinking: message.thinking,
            thought_duration_ms: message.thought_duration_ms,
            tool_success: message.tool_success,
            tool_denied: message.tool_denied,
            tool_calls: tool_calls_out,
            images: images_ret,
            file_ids: file_ids_ret,
        })
    }
}

pub struct Chat {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    /// The model this chat is bound to, with its provider — always present.
    pub model_id: i64,
    pub provider: String,
    pub model: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub summary: Option<String>,
    pub summary_up_to_message_id: Option<i64>,
    /// Key facts (structured, append-only) for this chat. NULL until the first fold
    /// — same convention as `summary`. See `Agent::compact` and the `ChatFacts` struct.
    pub key_facts: Option<ChatFacts>,
}

pub struct Message {
    pub id: i64,
    pub chat_id: i64,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub created_at: DateTimeUtc,
    pub thinking: Option<String>,
    pub thought_duration_ms: Option<i64>,
    pub tool_success: Option<bool>,
    pub tool_denied: bool,
    pub tool_calls: Vec<ToolCallOut>,
    pub images: Vec<String>,
    pub file_ids: Vec<i64>,
}

#[derive(Clone)]
pub struct ToolCallOut {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

pub struct NewToolCall {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

pub struct NewMessage {
    pub chat_id: i64,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub thinking: Option<String>,
    pub thought_duration_ms: Option<i64>,
    pub tool_success: Option<bool>,
    pub tool_denied: bool,
    pub tool_calls: Vec<NewToolCall>,
    pub images: Vec<String>,
    pub file_ids: Vec<i64>,
}

/// Structured, append-only key facts for a chat — exact facts extracted from
/// compaction folds. Old entries are never re-sent to the model as text to be
/// rewritten; each fold only asks for new facts, and merging is deterministic
/// in Rust (append + dedupe). Erosion is impossible by construction.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChatFacts {
    /// One sentence: the user's core request for the whole chat.
    /// Set on the first fold, never modified afterwards.
    pub goal: Option<String>,
    /// Append-only durable facts: exact paths, names+versions, confirmed API
    /// idioms, decisions, constraints, config values, open/unresolved items.
    pub facts: Vec<String>,
}

impl ChatFacts {
    /// Whether this is the zero/empty state — used to decide whether to persist
    /// NULL (a pre-feature chat with no facts at all) versus a real struct with
    /// no facts yet on its first fold.
    pub fn is_empty(&self) -> bool {
        self.goal.is_none() && self.facts.is_empty()
    }
}

/// Wraps every SeaORM failure uniformly — nothing about which specific query failed
/// changes how the caller should react (there's no retry/fallback logic per-query-type),
/// so unlike `OllamaErrors` this doesn't need multiple variants for that case. `NotFound`
/// is separate since it maps to a different HTTP status (404, not 500) and isn't a
/// failure at all from the database's point of view.
pub enum ChatStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    Model(ModelStoreErrors),
    User(UserStoreErrors),
}

impl From<UserStoreErrors> for ChatStoreErrors {
    fn from(err: UserStoreErrors) -> Self {
        ChatStoreErrors::User(err)
    }
}

impl From<DbErr> for ChatStoreErrors {
    fn from(err: DbErr) -> Self {
        ChatStoreErrors::QueryFailed(err)
    }
}

impl From<ModelStoreErrors> for ChatStoreErrors {
    fn from(err: ModelStoreErrors) -> Self {
        ChatStoreErrors::Model(err)
    }
}

impl From<ChatStoreErrors> for ErrorService {
    fn from(err: ChatStoreErrors) -> Self {
        match err {
            ChatStoreErrors::QueryFailed(e) => {
                tracing::error!("chat store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            ChatStoreErrors::Model(e) => e.into(),
            ChatStoreErrors::User(e) => e.into(),
            ChatStoreErrors::NotFound => ErrorService::new(StatusCode::NOT_FOUND, "chat not found"),
        }
    }
}
