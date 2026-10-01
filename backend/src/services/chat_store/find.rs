//! Finding chats across all of a user's own, by their name or by what was said in them.

use std::collections::{HashMap, HashSet};

use sea_orm::{prelude::*, FromQueryResult, QueryOrder, QuerySelect, QueryTrait};

use super::entities::{chats, messages};
use super::{ilike, like_pattern, snippet_around, ChatStore, ChatStoreErrors, SNIPPET_CONTEXT_CHARS};

/// The most matching messages read for one search. Their text is read to cut each snippet, and a
/// list longer than this is more than anyone scans anyway.
const MAX_MESSAGE_ROWS: u64 = 500;

pub struct ChatFindMessage {
    pub id: i64,
    pub role: String,
    pub created_at: DateTimeUtc,
    /// The content before the match, trimmed to a snippet; `None` when the match starts the message
    pub before: Option<String>,
    pub matched: String,
    /// The content after the match, trimmed to a snippet; `None` when the match ends the message
    pub after: Option<String>,
}

pub struct ChatFindResult {
    pub chat_id: i64,
    pub name: String,
    pub updated_at: DateTimeUtc,
    pub name_matched: bool,
    /// How many messages of the chat match (of those read, see `MAX_MESSAGE_ROWS`)
    pub message_matches: u64,
    /// The newest matches, at most `messages_per_chat`
    pub messages: Vec<ChatFindMessage>,
}

#[derive(FromQueryResult)]
struct MatchRow {
    id: i64,
    chat_id: i64,
    role: String,
    created_at: DateTimeUtc,
    content: String,
}

impl ChatStore {
    /// The user's chats whose name contains `query` and, with `include_messages`, those with a
    /// user or assistant message containing it (case-insensitive), most recently active first.
    /// Only the chats the chat list shows: deleted, sub-agent and plugin chats are left out.
    pub async fn find_chats(
        &self,
        user_id: i64,
        query: &str,
        include_messages: bool,
        limit: usize,
        messages_per_chat: usize,
    ) -> Result<Vec<ChatFindResult>, ChatStoreErrors> {
        let pattern = like_pattern(query);
        let live = || Self::listed_chats(user_id);

        let mut by_chat: HashMap<i64, chats::Model> = live()
            .filter(ilike(chats::Column::Name.into_expr(), &pattern))
            .all(&self.db)
            .await?
            .into_iter()
            .map(|chat| (chat.id, chat))
            .collect();
        let name_matched: HashSet<i64> = by_chat.keys().copied().collect();

        let mut matches: HashMap<i64, Vec<MatchRow>> = HashMap::new();
        if include_messages {
            let rows = messages::Entity::find()
                .select_only()
                .columns([
                    messages::Column::Id,
                    messages::Column::ChatId,
                    messages::Column::Role,
                    messages::Column::CreatedAt,
                    messages::Column::Content,
                ])
                .filter(messages::Column::Role.is_in(["user".to_string(), "assistant".to_string()]))
                .filter(ilike(messages::Column::Content.into_expr(), &pattern))
                .filter(messages::Column::ChatId.in_subquery(
                    live().select_only().column(chats::Column::Id).into_query(),
                ))
                .order_by_desc(messages::Column::CreatedAt)
                .limit(MAX_MESSAGE_ROWS)
                .into_model::<MatchRow>()
                .all(&self.db)
                .await?;
            for row in rows {
                matches.entry(row.chat_id).or_default().push(row);
            }

            // The chats that matched only through their messages still need their names
            let missing: Vec<i64> = matches.keys().filter(|id| !by_chat.contains_key(id)).copied().collect();
            if !missing.is_empty() {
                for chat in live().filter(chats::Column::Id.is_in(missing)).all(&self.db).await? {
                    by_chat.insert(chat.id, chat);
                }
            }
        }

        let mut results: Vec<ChatFindResult> = by_chat
            .into_values()
            .map(|chat| {
                let rows = matches.remove(&chat.id).unwrap_or_default();
                ChatFindResult {
                    chat_id: chat.id,
                    name: chat.name,
                    updated_at: chat.updated_at,
                    name_matched: name_matched.contains(&chat.id),
                    message_matches: rows.len() as u64,
                    messages: rows
                        .into_iter()
                        .take(messages_per_chat)
                        .map(|row| {
                            // `ILIKE` folds case more broadly than the snippet's own search can, so a
                            // match it can't locate still shows the start of the message
                            let (before, matched, after) = snippet_around(&row.content, query, SNIPPET_CONTEXT_CHARS)
                                .unwrap_or_else(|| (None, row.content.chars().take(SNIPPET_CONTEXT_CHARS).collect(), None));
                            ChatFindMessage {
                                id: row.id,
                                role: row.role,
                                created_at: row.created_at,
                                before,
                                matched,
                                after,
                            }
                        })
                        .collect(),
                }
            })
            .collect();

        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        results.truncate(limit);
        Ok(results)
    }
}
