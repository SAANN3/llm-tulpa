use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chats")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// The owning user. Every chat belongs to exactly one user (multi-user isolation).
    pub user_id: i64,
    pub name: String,
    /// The model this chat is bound to (`llm_models.id`) — never null.
    pub model_id: i64,
    pub is_deleted: bool,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub summary: Option<String>,
    pub summary_up_to_message_id: Option<i64>,
    /// Key facts (structured, append-only, never rewritten) for this chat.
    /// NULL until the first fold — same convention as `summary`.
    /// Stored as a plain `JsonValue` so old binary code that doesn't know this
    /// field still round-trips through SeaORM without error; the chat_store layer
    /// serializes/deserializes the flat `ChatFacts` struct around it.
    pub key_facts: Option<serde_json::Value>,
    /// Ground-truth prompt token count from Ollama's last evaluated turn; NULL after compaction fold.
    pub last_prompt_tokens: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::messages::Entity")]
    Messages,
}

impl Related<super::messages::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Messages.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
