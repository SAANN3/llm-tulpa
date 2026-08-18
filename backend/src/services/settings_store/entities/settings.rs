use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "settings")]
pub struct Model {
    /// Always `1` — this table only ever holds a single row (see `migrate`'s `CHECK`
    /// constraint), so the id exists only to give the singleton row an address.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub name: String,
    /// UTC offset in whole hours (e.g. `-5`, `9`), not an IANA timezone name.
    pub timezone: i16,
    pub notifications_enabled: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
