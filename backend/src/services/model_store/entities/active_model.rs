use sea_orm::entity::prelude::*;

/// A read-only projection of `user_settings` down to the one column `ModelStore` needs — which
/// model a user picked as their default. The table itself belongs to `SettingsStore`; this
/// keeps `ModelStore` from depending on that store's entity (or the other way round) just to
/// resolve a default. Never written through.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    pub active_model_id: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
