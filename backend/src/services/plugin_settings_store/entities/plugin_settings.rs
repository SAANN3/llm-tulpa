use sea_orm::entity::prelude::*;

/// Per-user plugin configuration (table `user_plugins`). Keyed in queries by
/// `(user_id, plugin_name, plugin_subname)`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_plugins")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub plugin_name: String,
    pub plugin_subname: String,
    pub settings: Json,
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
