use sea_orm::entity::prelude::*;

/// One base64 image attached to a message, ordered by `position` (normalized out of the
/// old JSONB array on `messages`).
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "message_images")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub message_id: i64,
    pub position: i32,
    pub data: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
