mod entities;
mod migrate;

use std::collections::HashMap;

use axum::http::StatusCode;
use entities::{chats, messages, tool_calls};
use migrate::migrate;
use sea_orm::{
    prelude::*, sea_query::Expr, ActiveValue::Set, Database, DatabaseConnection, DbBackend,
    PaginatorTrait, QueryOrder, QuerySelect, Statement, TransactionError, TransactionTrait,
};

use crate::services::error::ErrorService;

/// Owns chat/message persistence. SeaORM entities (`entities::chats`, `::messages`,
/// `::tool_calls`) are private to this module on purpose — everything outside
/// `ChatStore` only ever sees the plain structs below (`Chat`, `Message`, ...), never a
/// SeaORM `Model`/`ActiveModel` directly, so this is the only place that needs to know
/// SeaORM exists at all.
pub struct ChatStore {
    db: DatabaseConnection,
}

impl ChatStore {
    /// Connects to Postgres, creating the target database if it doesn't exist yet, then
    /// runs `migrate`. Panics on any failure — there's no reasonable way to run this app
    /// without a working database, so a `Result` here would just get `.unwrap()`'d by
    /// every caller anyway. `base_url` is the connection string *without* a database
    /// name (e.g. `postgres://user:pass@host:5432`); `db_name` is separate because
    /// creating the database requires first connecting to Postgres's own default
    /// `postgres` admin database, which you can't do once `base_url` already points at a
    /// database that may not exist yet.
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
            // db_name is a config value (env var), never user input, so string
            // interpolation here is fine — Postgres identifiers can't be bound as query
            // parameters in the first place, parameterization only covers values.
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

    /// Fetches a chat by id — the one place that decides whether a chat is usable.
    /// Returns `NotFound` both when no row exists and when it's soft-deleted, so a
    /// deleted chat looks identical to a nonexistent one to every caller, and nothing
    /// else needs its own `is_deleted` check.
    pub async fn chat(&self, chat_id: i64) -> Result<Chat, ChatStoreErrors> {
        let chat = chats::Entity::find_by_id(chat_id)
            .one(&self.db)
            .await?
            .filter(|chat| !chat.is_deleted)
            .ok_or(ChatStoreErrors::NotFound)?;

        Ok(Self::to_chat(chat))
    }

    /// Non-deleted, non-plugin-owned chats, newest-active first, plus the total count
    /// for pagination — this is what the ordinary chat UI lists, so a plugin-owned chat
    /// (`plugin_name` set — see `chats::Model`) never shows up here. See
    /// `chats_by_plugin` for the plugin-scoped counterpart.
    pub async fn chats(&self, limit: u64, skip: u64) -> Result<(Vec<Chat>, u64), ChatStoreErrors> {
        let query = chats::Entity::find()
            .filter(chats::Column::IsDeleted.eq(false))
            .filter(chats::Column::PluginName.is_null());

        self.paginated_chats(query, limit, skip).await
    }

    /// Same shape and pagination as `chats`, scoped to one plugin instance's chats
    /// instead of the ordinary (non-plugin) ones.
    pub async fn chats_by_plugin(
        &self,
        plugin_name: &str,
        plugin_subname: &str,
        limit: u64,
        skip: u64,
    ) -> Result<(Vec<Chat>, u64), ChatStoreErrors> {
        let query = chats::Entity::find()
            .filter(chats::Column::IsDeleted.eq(false))
            .filter(chats::Column::PluginName.eq(plugin_name))
            .filter(chats::Column::PluginSubname.eq(plugin_subname));

        self.paginated_chats(query, limit, skip).await
    }

    /// Shared by `chats`/`chats_by_plugin` — counts, orders, and paginates an
    /// already-filtered query the same way for both.
    async fn paginated_chats(
        &self,
        query: Select<chats::Entity>,
        limit: u64,
        skip: u64,
    ) -> Result<(Vec<Chat>, u64), ChatStoreErrors> {
        let total = query.clone().count(&self.db).await?;

        let chats = query
            .order_by_desc(chats::Column::UpdatedAt)
            .limit(limit)
            .offset(skip)
            .all(&self.db)
            .await?
            .into_iter()
            .map(Self::to_chat)
            .collect();

        Ok((chats, total))
    }

    /// Maps a SeaORM row to the plain struct the rest of the app sees — deliberately
    /// drops `plugin_name`/`plugin_subname`/`plugin_chat_id` (see `chats::Model`'s doc
    /// comment for why those never leave `ChatStore`).
    fn to_chat(model: chats::Model) -> Chat {
        Chat {
            id: model.id,
            name: model.name,
            created_at: model.created_at,
            updated_at: model.updated_at,
            summary: model.summary,
            summary_up_to_message_id: model.summary_up_to_message_id,
        }
    }

    /// Messages for a chat, newest first — `skip` counts from the newest end, so
    /// `skip=0, limit=50` gets the latest 50, and `skip=50, limit=50` gets the next 50
    /// older ones. Each message includes whatever tool calls it made. Also returns the
    /// total message count in the chat, for pagination. Errs via `chat` if the chat
    /// doesn't exist or is deleted, before ever querying its messages.
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

        let messages = self.hydrate_messages(rows).await?;

        Ok((messages, total))
    }

    /// Every message after `after_message_id` (exclusive) for a chat, newest first, up
    /// to `limit` — same convention as `messages`, just filtered to what's newer than a
    /// given message instead of everything. Used to rebuild history from just after a
    /// compaction boundary (`Chat::summary_up_to_message_id`) instead of from the
    /// start; pass `0` for `after_message_id` to get everything (message ids start at 1).
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

    /// Shared by `messages`/`messages_after` — attaches each row's tool calls (one
    /// batched query for the whole page rather than one per message) and maps into the
    /// plain `Message` shape the rest of the app sees.
    async fn hydrate_messages(&self, rows: Vec<messages::Model>) -> Result<Vec<Message>, ChatStoreErrors> {
        let message_ids: Vec<i64> = rows.iter().map(|message| message.id).collect();
        let mut tool_calls = self.tool_calls_by_messages(message_ids).await?;

        Ok(rows
            .into_iter()
            .map(|message| {
                let tool_calls = tool_calls.remove(&message.id).unwrap_or_default();

                Message {
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
                    tool_calls,
                    images: message
                        .images
                        .and_then(|images| serde_json::from_value(images).ok())
                        .unwrap_or_default(),
                }
            })
            .collect())
    }

    /// Fetches every tool call for a batch of messages in one query, grouped by
    /// `message_id` and ordered by `id` within each group — that insertion order is the
    /// same order the model requested the calls in (their `index` in Ollama's
    /// `tool_calls` array), which callers need to resolve "which of these tool calls is
    /// next" without a separate ordering column. A real single-query `LEFT JOIN`
    /// (Postgres `json_agg` + `GROUP BY`, or SeaORM's `find_with_related`) is possible
    /// but doesn't compose cleanly with the `LIMIT`/`OFFSET` pagination `messages`
    /// already applies to the message side — a join would cap joined (message,
    /// tool_call) *pairs*, not distinct messages. Two queries plus grouping in Rust
    /// sidesteps that at negligible cost for this scale.
    async fn tool_calls_by_messages(
        &self,
        message_ids: Vec<i64>,
    ) -> Result<HashMap<i64, Vec<ToolCallOut>>, ChatStoreErrors> {
        let calls = tool_calls::Entity::find()
            .filter(tool_calls::Column::MessageId.is_in(message_ids))
            .order_by_asc(tool_calls::Column::Id)
            .all(&self.db)
            .await?;

        let mut grouped: HashMap<i64, Vec<ToolCallOut>> = HashMap::new();
        for call in calls {
            grouped
                .entry(call.message_id)
                .or_default()
                .push(ToolCallOut {
                    tool_name: call.tool_name,
                    arguments: call.arguments,
                });
        }

        Ok(grouped)
    }

    /// Creates an ordinary (non-plugin) chat and returns the full row — including
    /// `created_at`/`updated_at`, which Postgres just generated, so there's no reason to
    /// make the caller guess or regenerate them.
    pub async fn create_chat(&self, name: String) -> Result<Chat, ChatStoreErrors> {
        self.insert_chat(name, None, None, None).await
    }

    /// Creates a chat owned by one plugin instance, mapped to that plugin's own
    /// `plugin_chat_id` (e.g. a Telegram chat id) — see `chats::Model`'s doc comment for
    /// the all-or-nothing constraint this relies on.
    pub async fn create_plugin_chat(
        &self,
        name: String,
        plugin_name: String,
        plugin_subname: String,
        plugin_chat_id: String,
    ) -> Result<Chat, ChatStoreErrors> {
        self.insert_chat(name, Some(plugin_name), Some(plugin_subname), Some(plugin_chat_id))
            .await
    }

    /// Shared by `create_chat`/`create_plugin_chat`.
    async fn insert_chat(
        &self,
        name: String,
        plugin_name: Option<String>,
        plugin_subname: Option<String>,
        plugin_chat_id: Option<String>,
    ) -> Result<Chat, ChatStoreErrors> {
        let chat = chats::ActiveModel {
            name: Set(name),
            plugin_name: Set(plugin_name),
            plugin_subname: Set(plugin_subname),
            plugin_chat_id: Set(plugin_chat_id),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;

        Ok(Self::to_chat(chat))
    }

    /// Looks up the chat mapped to one plugin instance's external chat id (e.g. a
    /// Telegram chat id) — `None` if no chat has been mapped to it yet (not deleted
    /// either, same as `chat`). Used to turn an incoming plugin message's chat id back
    /// into the internal `Chat` it belongs to.
    pub async fn find_by_plugin_mapped_id(
        &self,
        plugin_name: &str,
        plugin_subname: &str,
        plugin_chat_id: &str,
    ) -> Result<Option<Chat>, ChatStoreErrors> {
        let chat = chats::Entity::find()
            .filter(chats::Column::IsDeleted.eq(false))
            .filter(chats::Column::PluginName.eq(plugin_name))
            .filter(chats::Column::PluginSubname.eq(plugin_subname))
            .filter(chats::Column::PluginChatId.eq(plugin_chat_id))
            .one(&self.db)
            .await?;

        Ok(chat.map(Self::to_chat))
    }

    /// `find_by_plugin_mapped_id`, creating a new plugin chat (via `name`) the first
    /// time this external chat id is seen instead of returning `None` — the usual entry
    /// point for an incoming plugin message, where there's always a chat to hand back
    /// either way.
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
    /// history. This is replay bookkeeping, not conversational content — nothing about
    /// the chat's actual message rows changes.
    pub async fn set_summary(&self, chat_id: i64, summary: String, up_to_message_id: i64) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;

        chats::ActiveModel {
            id: Set(chat_id),
            summary: Set(Some(summary)),
            summary_up_to_message_id: Set(Some(up_to_message_id)),
            ..Default::default()
        }
        .update(&self.db)
        .await?;

        Ok(())
    }

    /// Renames the given chat. Errs via `chat` if it doesn't exist or is deleted, before
    /// ever issuing the update.
    pub async fn rename_chat(&self, chat_id: i64, name: String) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;

        chats::ActiveModel {
            id: Set(chat_id),
            name: Set(name),
            ..Default::default()
        }
        .update(&self.db)
        .await?;

        Ok(())
    }

    /// Soft delete — marks the chat `is_deleted` rather than removing it (and its
    /// messages) outright, so `chats()` just excludes it going forward. Errs via `chat`
    /// if it doesn't exist or is already deleted, before ever issuing the update.
    pub async fn delete_chat(&self, chat_id: i64) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;

        chats::ActiveModel {
            id: Set(chat_id),
            is_deleted: Set(true),
            ..Default::default()
        }
        .update(&self.db)
        .await?;

        Ok(())
    }

    /// Deletes every message (and their tool calls) belonging to a chat, and clears its
    /// compaction summary — unlike `delete_chat`, the chat row itself (its id, name, and
    /// for a plugin chat, its external mapping) is left untouched, so the same
    /// conversation keeps working with a clean slate instead of losing its identity.
    /// Deliberately not built on `delete_chat`'s soft-delete: a plugin chat's
    /// `(plugin_name, plugin_subname, plugin_chat_id)` triple is uniquely constrained
    /// across *all* rows regardless of `is_deleted`, so soft-deleting one would permanently
    /// block that same external chat from ever being linked again. Errs via `chat` if the
    /// chat doesn't exist or is already deleted, before touching anything.
    pub async fn clear_messages(&self, chat_id: i64) -> Result<(), ChatStoreErrors> {
        self.chat(chat_id).await?;

        self.db
            .transaction::<_, (), DbErr>(|txn| {
                Box::pin(async move {
                    let message_ids: Vec<i64> = messages::Entity::find()
                        .filter(messages::Column::ChatId.eq(chat_id))
                        .select_only()
                        .column(messages::Column::Id)
                        .into_tuple()
                        .all(txn)
                        .await?;

                    if !message_ids.is_empty() {
                        tool_calls::Entity::delete_many()
                            .filter(tool_calls::Column::MessageId.is_in(message_ids))
                            .exec(txn)
                            .await?;
                    }

                    messages::Entity::delete_many()
                        .filter(messages::Column::ChatId.eq(chat_id))
                        .exec(txn)
                        .await?;

                    chats::ActiveModel {
                        id: Set(chat_id),
                        summary: Set(None),
                        summary_up_to_message_id: Set(None),
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

    /// Inserts a message and its tool calls, and bumps the parent chat's `updated_at` —
    /// three writes, wrapped in one transaction so a failure partway through (e.g. a
    /// tool_calls insert failing) can't leave a message stored with no record of what it
    /// called, or a chat's `updated_at` out of sync with its actual latest message.
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
        } = new_message;

        // Stored as `NULL` rather than `[]` for a message with none — same convention
        // as `summary`/`thinking` above, and keeps every pre-existing row (which has no
        // `images` at all) indistinguishable from one explicitly sent with zero images.
        let images_json = (!images.is_empty()).then(|| serde_json::json!(images));

        let tool_calls_out: Vec<ToolCallOut> = tool_calls
            .iter()
            .map(|call| ToolCallOut {
                tool_name: call.tool_name.clone(),
                arguments: call.arguments.clone(),
            })
            .collect();

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
                        images: Set(images_json),
                        ..Default::default()
                    }
                    .insert(txn)
                    .await?;

                    for tool_call in tool_calls {
                        tool_calls::ActiveModel {
                            message_id: Set(message.id),
                            tool_name: Set(tool_call.tool_name),
                            arguments: Set(tool_call.arguments),
                            ..Default::default()
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
                TransactionError::Connection(e) | TransactionError::Transaction(e) => {
                    ChatStoreErrors::from(e)
                }
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
            images,
        })
    }
}

pub struct Chat {
    pub id: i64,
    pub name: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    /// A rolling summary covering every message up to and including
    /// `summary_up_to_message_id`, once this chat's history has crossed the
    /// compaction threshold at least once — `None` until then. See `Agent::compact`.
    pub summary: Option<String>,
    pub summary_up_to_message_id: Option<i64>,
}

pub struct Message {
    pub id: i64,
    pub chat_id: i64,
    pub role: String,
    pub content: String,
    /// Only present on `tool`-role messages — which tool produced `content`.
    pub tool_name: Option<String>,
    pub created_at: DateTimeUtc,
    /// Only present on `assistant`-role messages generated with `think: true`.
    pub thinking: Option<String>,
    /// The full Ollama call's duration in milliseconds, not just the `<think>` portion
    /// — see the same field on `NewMessage` for why. Set alongside `thinking`.
    pub thought_duration_ms: Option<i64>,
    /// Only present on `tool`-role messages — see `NewMessage::tool_success`.
    pub tool_success: Option<bool>,
    /// Only meaningful on a `tool`-role message with `tool_success: Some(false)` — see
    /// `NewMessage::tool_denied`.
    pub tool_denied: bool,
    pub tool_calls: Vec<ToolCallOut>,
    /// Base64-encoded image data attached to this message (no data-URL prefix), if
    /// any. Empty for every role but `user`.
    pub images: Vec<String>,
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

/// Bundled rather than separate positional args on `new_message` — `role` and `content`
/// sitting next to each other as bare `String`s is exactly the same-type-argument mixup
/// risk a struct avoids for free.
pub struct NewMessage {
    pub chat_id: i64,
    pub role: String,
    pub content: String,
    /// Only set for `tool`-role messages — which tool produced `content`.
    pub tool_name: Option<String>,
    pub thinking: Option<String>,
    /// Wall-clock time the Ollama call that produced this message took, in
    /// milliseconds. Covers the whole call (thinking and answer generation together)
    /// rather than isolating just the `<think>` portion — Ollama's non-streaming
    /// responses don't report those separately, and splitting them out for real would
    /// need switching that call to streaming and watching for the `</think>` boundary
    /// as it arrives, which is a bigger change than this field is worth yet.
    pub thought_duration_ms: Option<i64>,
    /// Mirrors `UseToolOut.success` at write time — set for a `tool`-role message,
    /// `None` for every other role. Persisted so a historical `tool` message can be
    /// rendered the same way a live one is, without parsing `content` to guess.
    pub tool_success: Option<bool>,
    /// Mirrors `UseToolOut.denied` at write time — only meaningful alongside
    /// `tool_success: Some(false)`; pass `false` for every other case (including every
    /// non-`tool`-role message).
    pub tool_denied: bool,
    pub tool_calls: Vec<NewToolCall>,
    /// Base64-encoded image data (no data-URL prefix) to attach to this message. Only
    /// meaningful on a `user`-role message — pass empty for every other role.
    pub images: Vec<String>,
}

/// Wraps every SeaORM failure uniformly — nothing about which specific query failed
/// changes how the caller should react (there's no retry/fallback logic per-query-type),
/// so unlike `OllamaErrors` this doesn't need multiple variants for that case. `NotFound`
/// is separate since it maps to a different HTTP status (404, not 500) and isn't a
/// failure at all from the database's point of view.
pub enum ChatStoreErrors {
    QueryFailed(DbErr),
    NotFound,
}

impl From<DbErr> for ChatStoreErrors {
    fn from(err: DbErr) -> Self {
        ChatStoreErrors::QueryFailed(err)
    }
}

impl From<ChatStoreErrors> for ErrorService {
    fn from(err: ChatStoreErrors) -> Self {
        match err {
            ChatStoreErrors::QueryFailed(e) => {
                tracing::error!("chat store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            ChatStoreErrors::NotFound => ErrorService::new(StatusCode::NOT_FOUND, "chat not found"),
        }
    }
}
