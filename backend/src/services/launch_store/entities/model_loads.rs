use sea_orm::entity::prelude::*;

/// One start of the model server and how long it took to be ready — see `LaunchStore::record_load`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "model_loads")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub model_id: Option<i64>,
    pub profile_id: Option<i64>,
    pub started_at: DateTimeUtc,
    pub duration_ms: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
