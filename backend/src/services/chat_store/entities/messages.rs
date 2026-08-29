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
    pub created_at: DateTimeUtc,
    /// The model's reasoning trace, when this message was generated with `think: true`
    /// and the model produced one. Only ever set on `assistant`-role messages.
    pub thinking: Option<String>,
    /// How long the Ollama call that produced this message took, end to end — not
    /// isolated to just the `<think>` portion (see `Agent::advance`), since Ollama's
    /// non-streaming responses don't report thinking and answer generation separately.
    pub thought_duration_ms: Option<i64>,
    /// Only meaningful on a `tool`-role row — mirrors `UseToolOut.success` at the time
    /// this message was written, so a historical `tool` message can be rendered the
    /// same way a live one is, without parsing `content`. `None` on every other role.
    pub tool_success: Option<bool>,
    /// Only meaningful on a `tool`-role row with `tool_success: Some(false)` — mirrors
    /// `UseToolOut.denied`, distinguishing a call the tool actually ran and failed from
    /// one that never ran because it wasn't permitted. `false` (not meaningful) on
    /// every other row.
    pub tool_denied: bool,
    /// Base64-encoded image data attached to this message (no data-URL prefix), if
    /// any. Only ever set on `user`-role rows.
    pub images: Option<Json>,
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
