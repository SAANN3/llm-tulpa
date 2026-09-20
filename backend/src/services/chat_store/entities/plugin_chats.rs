use sea_orm::entity::prelude::*;

/// The plugin-owned-chat mapping, normalized out of the old nullable plugin columns on
/// `chats`. Present only for a chat that a messaging plugin owns; 1:1 with `chats`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_chats")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub chat_id: i64,
    pub plugin_name: String,
    pub plugin_subname: String,
    pub plugin_chat_id: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
