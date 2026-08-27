use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_settings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub plugin_name: String,
    pub plugin_subname: String,
    pub settings: Json,
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
