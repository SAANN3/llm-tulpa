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
    /// The model's reasoning trace, when it produced one. Only on `assistant` rows.
    pub thinking: Option<String>,
    /// How long the Ollama call that produced this message took, end to end — not isolated to
    /// the thinking part, since non-streaming responses don't report the two separately.
    pub thought_duration_ms: Option<i64>,
    /// `tool` rows only: whether the tool succeeded, so a historical message renders like a live
    /// one without parsing `content`.
    pub tool_success: Option<bool>,
    /// `tool` rows with `tool_success: Some(false)` only: the call never ran because it wasn't
    /// permitted, as opposed to running and failing.
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
