use sea_orm::entity::prelude::*;

/// A model the app knows about, registered when a user picks or pulls one.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "llm_models")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub provider_id: i64,
    pub name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::llm_providers::Entity",
        from = "Column::ProviderId",
        to = "super::llm_providers::Column::Id"
    )]
    Provider,
}

impl Related<super::llm_providers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Provider.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
