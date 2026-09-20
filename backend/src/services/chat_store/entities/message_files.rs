use sea_orm::entity::prelude::*;

/// Junction between a message and an attached file, ordered by `position` (normalized
/// out of the old JSONB id array on `messages`). Composite primary key.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "message_files")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub message_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub file_id: i64,
    pub position: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
