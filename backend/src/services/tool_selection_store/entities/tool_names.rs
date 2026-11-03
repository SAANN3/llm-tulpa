use sea_orm::entity::prelude::*;

/// One tool name in a chat's or a user's selection. Exactly one of `chat_id` and `user_id` is set.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tool_names")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub chat_id: Option<i64>,
    pub user_id: Option<i64>,
    pub name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
