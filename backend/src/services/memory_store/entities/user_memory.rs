use sea_orm::entity::prelude::*;

/// One thing remembered about a user — see `MemoryStore`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_memory")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    /// `about_you`, `preferences` or `misc` — see `Category`, the only thing that reads or writes it as a bare string
    pub category: String,
    pub text: String,
    /// The user's own words the entry was learned from; NULL for one the user wrote or edited
    pub quote: Option<String>,
    /// Written by the model (it may replace or drop it) rather than by the user
    pub learned: bool,
    /// The chat the entry was learned in; NULL for the user's own entries and once that chat is deleted
    pub source_chat_id: Option<i64>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
