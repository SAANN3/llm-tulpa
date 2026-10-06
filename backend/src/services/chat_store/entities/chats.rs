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
    /// The folder this chat is grouped under (`folders.id`), or NULL if ungrouped.
    pub folder_id: Option<i64>,
    /// The chat that delegated to this one (`chats.id`), or NULL for an ordinary chat.
    pub parent_chat_id: Option<i64>,
    /// The launch profile this chat runs on (`launch_profiles.id`), which implies its model; NULL for
    /// a chat on a model that has none.
    pub launch_profile_id: Option<i64>,
    /// The agent's own working notes (`chat.write_notes`), kept across compaction folds; NULL until written.
    pub notes: Option<String>,
    /// What `chat.write_notes` saved since the last compaction. It stays out of the prompt (the text is in the
    /// tool call the model made) and replaces `notes` at the next compaction; changing the notes in the prompt
    /// would make the model server read the whole prompt again.
    pub notes_pending: Option<String>,
    /// Tool results up to and including this message id are shown as one-line stubs; NULL when none are.
    pub cleared_up_to_message_id: Option<i64>,
    /// Thinking traces up to and including this message id are replayed as their tail only; NULL when none are.
    pub thinking_trimmed_up_to_message_id: Option<i64>,
    /// Whether the model is sent its tools in this chat. A model too small for them, or a window the tool
    /// definitions would mostly fill, runs without; a new chat takes the user's default.
    pub tools_enabled: bool,
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
