mod entities;
pub mod merge;

use axum::http::StatusCode;
use entities::user_memory;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, DbErr, QueryOrder, TransactionTrait};

use crate::services::error::ErrorService;
use merge::{Candidate, Decision};

/// Owns what is remembered about each user across chats: short entries, some the model learned from the user's own words
/// when a chat folded and some the user wrote themselves. The model's may be replaced when the user corrects themselves
/// and are the only ones dropped for the user's limit; an entry the user wrote or edited is never touched by the model.
/// What reaches a chat's prompt is a snapshot of it (`merge::snapshot_text`), taken when the chat starts and at each fold.
pub struct MemoryStore {
    db: DatabaseConnection,
}

/// The longest entry a user may write themselves
const MAX_USER_ENTRY_CHARS: usize = 1000;

/// What an entry is about. Three broad kinds, since a memory spanning every chat has no use for fine ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Who the user is: role, setup, hardware, languages, what they generally work on
    AboutYou,
    /// How they like things done or answered
    Preferences,
    /// Anything else lasting
    Misc,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::AboutYou => "about_you",
            Category::Preferences => "preferences",
            Category::Misc => "misc",
        }
    }

    /// `None` for a word that is none of the three; the model's reply is read with `about_you` for those.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "about_you" => Some(Category::AboutYou),
            "preferences" => Some(Category::Preferences),
            "misc" => Some(Category::Misc),
            _ => None,
        }
    }
}

/// One entry, decoupled from the private SeaORM `user_memory::Model`.
#[derive(Clone, Debug, PartialEq)]
pub struct MemoryEntry {
    pub id: i64,
    pub category: Category,
    pub text: String,
    pub quote: Option<String>,
    pub learned: bool,
    pub source_chat_id: Option<i64>,
    pub updated_at: DateTimeUtc,
}

impl From<user_memory::Model> for MemoryEntry {
    fn from(m: user_memory::Model) -> Self {
        Self {
            id: m.id,
            category: Category::parse(&m.category).unwrap_or(Category::Misc),
            text: m.text,
            quote: m.quote,
            learned: m.learned,
            source_chat_id: m.source_chat_id,
            updated_at: m.updated_at,
        }
    }
}

/// What a learning pass came to, for the step the page shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Learned {
    pub added: usize,
    pub replaced: usize,
    pub skipped: usize,
    /// The model's entries dropped to keep to the user's limit
    pub dropped: usize,
}

impl MemoryStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// All the user's entries, the most recently changed first.
    pub async fn list(&self, user_id: i64) -> Result<Vec<MemoryEntry>, MemoryStoreErrors> {
        Ok(user_memory::Entity::find()
            .filter(user_memory::Column::UserId.eq(user_id))
            .order_by_desc(user_memory::Column::UpdatedAt)
            .order_by_desc(user_memory::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(MemoryEntry::from)
            .collect())
    }

    /// An entry the user writes: never replaced or dropped by the model.
    pub async fn add(&self, user_id: i64, category: Category, text: &str) -> Result<MemoryEntry, MemoryStoreErrors> {
        let text = checked_text(text)?;
        let row = user_memory::ActiveModel {
            user_id: Set(user_id),
            category: Set(category.as_str().to_string()),
            text: Set(text),
            quote: Set(None),
            learned: Set(false),
            source_chat_id: Set(None),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(row.into())
    }

    /// Changes an entry's category and text. The entry is the user's own from then on, and its quote, which no longer
    /// has to support what it says, is dropped.
    pub async fn update(&self, user_id: i64, id: i64, category: Category, text: &str) -> Result<MemoryEntry, MemoryStoreErrors> {
        let text = checked_text(text)?;
        let row = self.owned(user_id, id).await?;
        let mut model: user_memory::ActiveModel = row.into();
        model.category = Set(category.as_str().to_string());
        model.text = Set(text);
        model.quote = Set(None);
        model.learned = Set(false);
        model.updated_at = Set(chrono::Utc::now());
        Ok(model.update(&self.db).await?.into())
    }

    pub async fn delete(&self, user_id: i64, id: i64) -> Result<(), MemoryStoreErrors> {
        let deleted = user_memory::Entity::delete_many()
            .filter(user_memory::Column::Id.eq(id))
            .filter(user_memory::Column::UserId.eq(user_id))
            .exec(&self.db)
            .await?;
        if deleted.rows_affected == 0 {
            return Err(MemoryStoreErrors::NotFound);
        }
        Ok(())
    }

    /// Removes everything remembered about the user; how many entries there were.
    pub async fn clear(&self, user_id: i64) -> Result<u64, MemoryStoreErrors> {
        Ok(user_memory::Entity::delete_many().filter(user_memory::Column::UserId.eq(user_id)).exec(&self.db).await?.rows_affected)
    }

    /// Takes the candidates a model proposed into the user's memory (see `merge::plan`), learned in `chat_id`, and then drops
    /// the model's oldest entries beyond `max_learned`. One transaction: a failure leaves the memory as it was.
    pub async fn learn(&self, user_id: i64, chat_id: i64, candidates: Vec<Candidate>, max_learned: usize) -> Result<Learned, MemoryStoreErrors> {
        let txn = self.db.begin().await?;
        let existing: Vec<MemoryEntry> = user_memory::Entity::find()
            .filter(user_memory::Column::UserId.eq(user_id))
            .all(&txn)
            .await?
            .into_iter()
            .map(MemoryEntry::from)
            .collect();
        let mut outcome = Learned::default();
        for decision in merge::plan(&existing, candidates) {
            match decision {
                Decision::Add { category, text, quote } => {
                    user_memory::ActiveModel {
                        user_id: Set(user_id),
                        category: Set(category.as_str().to_string()),
                        text: Set(text),
                        quote: Set(Some(quote)),
                        learned: Set(true),
                        source_chat_id: Set(Some(chat_id)),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await?;
                    outcome.added += 1;
                }
                Decision::Replace { id, category, text, quote } => {
                    user_memory::ActiveModel {
                        id: Set(id),
                        category: Set(category.as_str().to_string()),
                        text: Set(text),
                        quote: Set(Some(quote)),
                        source_chat_id: Set(Some(chat_id)),
                        updated_at: Set(chrono::Utc::now()),
                        ..Default::default()
                    }
                    .update(&txn)
                    .await?;
                    outcome.replaced += 1;
                }
                Decision::Skip(_) => outcome.skipped += 1,
            }
        }
        let learned_ids: Vec<i64> = user_memory::Entity::find()
            .filter(user_memory::Column::UserId.eq(user_id))
            .filter(user_memory::Column::Learned.eq(true))
            .order_by_asc(user_memory::Column::UpdatedAt)
            .order_by_asc(user_memory::Column::Id)
            .all(&txn)
            .await?
            .into_iter()
            .map(|row| row.id)
            .collect();
        if learned_ids.len() > max_learned {
            let drop = &learned_ids[..learned_ids.len() - max_learned];
            outcome.dropped = drop.len();
            user_memory::Entity::delete_many().filter(user_memory::Column::Id.is_in(drop.to_vec())).exec(&txn).await?;
        }
        txn.commit().await?;
        Ok(outcome)
    }

    /// The text a chat's system prompt takes from the user's memory, within `budget` characters; empty when there is none.
    pub async fn snapshot(&self, user_id: i64, budget: usize) -> Result<String, MemoryStoreErrors> {
        Ok(merge::snapshot_text(&self.list(user_id).await?, budget))
    }

    async fn owned(&self, user_id: i64, id: i64) -> Result<user_memory::Model, MemoryStoreErrors> {
        user_memory::Entity::find_by_id(id)
            .filter(user_memory::Column::UserId.eq(user_id))
            .one(&self.db)
            .await?
            .ok_or(MemoryStoreErrors::NotFound)
    }
}

fn checked_text(text: &str) -> Result<String, MemoryStoreErrors> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err(MemoryStoreErrors::Invalid("an entry needs some text".to_string()));
    }
    if text.chars().count() > MAX_USER_ENTRY_CHARS {
        return Err(MemoryStoreErrors::Invalid(format!("an entry may be {MAX_USER_ENTRY_CHARS} characters at most")));
    }
    Ok(text)
}

#[derive(Debug)]
pub enum MemoryStoreErrors {
    QueryFailed(DbErr),
    NotFound,
    Invalid(String),
}

impl From<DbErr> for MemoryStoreErrors {
    fn from(err: DbErr) -> Self {
        MemoryStoreErrors::QueryFailed(err)
    }
}

impl From<MemoryStoreErrors> for ErrorService {
    fn from(err: MemoryStoreErrors) -> Self {
        match err {
            MemoryStoreErrors::QueryFailed(e) => {
                tracing::error!("memory store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            MemoryStoreErrors::NotFound => ErrorService::new(StatusCode::NOT_FOUND, "no such memory entry"),
            MemoryStoreErrors::Invalid(why) => ErrorService::new(StatusCode::BAD_REQUEST, why),
        }
    }
}
