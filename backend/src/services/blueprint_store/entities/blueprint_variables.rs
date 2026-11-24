use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "blueprint_variables")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub blueprint_id: i64,
    pub values: Json,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
