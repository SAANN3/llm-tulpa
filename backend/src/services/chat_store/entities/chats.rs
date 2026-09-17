use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "chats")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
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
    /// All three null (an ordinary chat) or all three filled (a plugin-owned one) —
    /// see `chat_store::migrate` for the constraint that enforces this. Deliberately
    /// not on the public `Chat` struct — nothing outside `ChatStore` needs to know a
    /// chat came from a plugin, only `ChatStore` itself (`chats`/`chats_by_plugin`,
    /// `create_chat`/`create_plugin_chat`) ever filters or sets these.
    pub plugin_name: Option<String>,
    pub plugin_subname: Option<String>,
    pub plugin_chat_id: Option<String>,
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
