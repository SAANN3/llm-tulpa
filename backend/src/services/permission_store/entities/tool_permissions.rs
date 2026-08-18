use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tool_permissions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub chat_id: i64,
    pub tool_name: String,
    pub scope: Json,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
