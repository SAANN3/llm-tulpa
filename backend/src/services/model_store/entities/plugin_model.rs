use sea_orm::entity::prelude::*;

/// The model (and launch profile) an owner chose for one plugin's chats. No row means the plugin has no choice.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_models")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub plugin_name: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub plugin_subname: String,
    pub model_id: i64,
    pub launch_profile_id: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
