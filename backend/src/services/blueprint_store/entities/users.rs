use sea_orm::entity::prelude::*;

// Only the identity is projected: locking it serializes one user's queue across all blueprints.
// User creation, updates and deletion remain owned by UserStore.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
