use sea_orm::entity::prelude::*;

/// An external plugin the backend has seen (table `external_plugins`), keyed by the `(name, subname)` of its folder.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "external_plugins")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub subname: String,
    pub version: Option<String>,
    pub kind: Option<String>,
    pub source_type: String,
    pub source_url: Option<String>,
    pub installed_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub state: String,
    pub last_error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
