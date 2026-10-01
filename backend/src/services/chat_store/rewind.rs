//! Cutting a chat back to just before one of its messages.

use sea_orm::{prelude::*, QuerySelect, QueryTrait, TransactionError, TransactionTrait};

use super::entities::{chats, messages, plugin_chats, tool_calls};
use super::{ChatStore, ChatStoreErrors};

impl ChatStore {
    /// Deletes `message_id` and every message after it, returning how many went, so the chat reads
    /// as it did before that message. Refused with `Conflict` unless the whole cut is plain
    /// conversation: a tool call, a tool result or a job notice anywhere in it would leave their
    /// effects (a written file, a started job) with nothing in the chat to explain them. Also
    /// refused in a sub-agent's chat and a plugin's chat, whose messages have already gone out
    /// elsewhere, and for a message the compaction summary has already folded in, which the model
    /// is no longer sent.
    ///
    /// The checks and the delete share one transaction, so a message that arrives in between
    /// (another tab, a finished job) can't slip into the cut unchecked. The last evaluated prompt
    /// size is cleared with it: it described the longer history, and the compaction decision reads
    /// it.
    pub async fn rewind_from(&self, chat_id: i64, message_id: i64) -> Result<u64, ChatStoreErrors> {
        let refuse = |why: &str| ChatStoreErrors::Conflict(format!("that can't be removed: {why}"));

        // A refusal is the transaction's *value*, not its error: nothing has been written by then,
        // so committing it is harmless, and only a database failure rolls back as an error.
        self.db
            .transaction::<_, Result<u64, ChatStoreErrors>, DbErr>(|txn| {
                Box::pin(async move {
                    let Some(chat) = chats::Entity::find_by_id(chat_id).one(txn).await? else {
                        return Ok(Err(ChatStoreErrors::NotFound));
                    };
                    if chat.parent_chat_id.is_some() {
                        return Ok(Err(refuse("a sub-agent's chat runs on its own")));
                    }
                    if plugin_chats::Entity::find_by_id(chat_id).one(txn).await?.is_some() {
                        return Ok(Err(refuse("its messages have already been sent to a messaging app")));
                    }

                    let Some(target) = messages::Entity::find_by_id(message_id)
                        .filter(messages::Column::ChatId.eq(chat_id))
                        .one(txn)
                        .await?
                    else {
                        // Also what a second click or a stale tab runs into
                        return Ok(Err(refuse("that message is no longer in the chat")));
                    };
                    if chat.summary_up_to_message_id.is_some_and(|boundary| target.id <= boundary) {
                        return Ok(Err(refuse("it is already folded into the chat's summary")));
                    }

                    let cut = || {
                        messages::Entity::find()
                            .filter(messages::Column::ChatId.eq(chat_id))
                            .filter(messages::Column::Id.gte(message_id))
                    };
                    let non_conversational = cut()
                        .filter(messages::Column::Role.is_not_in(["user", "assistant"]))
                        .count(txn)
                        .await?;
                    let with_tool_calls = tool_calls::Entity::find()
                        .filter(tool_calls::Column::MessageId.in_subquery(
                            cut().select_only().column(messages::Column::Id).into_query(),
                        ))
                        .count(txn)
                        .await?;
                    if non_conversational > 0 || with_tool_calls > 0 {
                        return Ok(Err(refuse("a tool was used from there on")));
                    }

                    let deleted = messages::Entity::delete_many()
                        .filter(messages::Column::ChatId.eq(chat_id))
                        .filter(messages::Column::Id.gte(message_id))
                        .exec(txn)
                        .await?;

                    chats::ActiveModel {
                        id: sea_orm::Set(chat_id),
                        last_prompt_tokens: sea_orm::Set(None),
                        ..Default::default()
                    }
                    .update(txn)
                    .await?;

                    Ok(Ok(deleted.rows_affected))
                })
            })
            .await
            .map_err(|err| match err {
                TransactionError::Connection(e) | TransactionError::Transaction(e) => ChatStoreErrors::from(e),
            })?
    }
}
