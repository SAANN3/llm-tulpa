use sea_orm::entity::prelude::*;

/// A prompt accepted while its chat's turn was still going. It is not a message yet: the run in
/// progress must not see it. The oldest row (lowest `position`) starts once that turn finishes.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "queued_prompts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub chat_id: i64,
    pub position: i32,
    pub prompt: String,
    pub images: Json,
    pub file_ids: Json,
    pub think: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
