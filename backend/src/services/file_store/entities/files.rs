use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "files")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// `None` for a file uploaded before a chat existed to attach it to yet (the home
    /// page's case) — claimed (set to a real chat) the moment it's actually attached
    /// to a message. See `FileStore::attach_to_chat`.
    pub chat_id: Option<i64>,
    pub full_path: String,
    pub file_name: String,
    pub read_only: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
