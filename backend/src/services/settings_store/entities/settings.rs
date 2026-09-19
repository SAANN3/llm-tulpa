use sea_orm::entity::prelude::*;

/// Per-user settings, 1:1 with `users` (replaces the old single global row). The primary
/// key is the owning `user_id`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    pub name: Option<String>,
    /// UTC offset in whole hours (e.g. `-5`, `9`), not an IANA timezone name.
    pub timezone: Option<i16>,
    pub notifications_enabled: bool,
    pub theme: Option<String>,
    pub language: String,
    /// The model this user's new chats default to (`llm_models.id`). The provider is
    /// reached through the model, not stored here.
    pub active_model_id: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
