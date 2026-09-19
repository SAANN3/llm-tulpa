use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "messages")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub chat_id: i64,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub thinking: Option<String>,
    pub thought_duration_ms: Option<i64>,
    pub tool_success: Option<bool>,
    pub tool_denied: bool,
    pub created_at: DateTimeUtc,
    // Attached images/files are normalized into `message_images`/`message_files` (3NF);
    // `ChatStore` reassembles them onto the public `Message` struct.
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::chats::Entity",
        from = "Column::ChatId",
        to = "super::chats::Column::Id"
    )]
    Chat,
    #[sea_orm(has_many = "super::tool_calls::Entity")]
    ToolCalls,
}

impl Related<super::chats::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Chat.def()
    }
}

impl Related<super::tool_calls::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ToolCalls.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
