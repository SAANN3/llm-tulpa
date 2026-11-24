use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "blueprint_agent_chats")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub blueprint_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub node_id: String,
    pub chat_id: i64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
