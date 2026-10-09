use std::sync::Arc;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    facade::agent::{ContextPart, ContextPartKind},
    routes::auth::AuthUser,
    services::error::ErrorService,
    state::AppState,
};

#[derive(Deserialize, IntoParams)]
pub(crate) struct ChatContextQuery {
    chat_id: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContextPartKindOut {
    /// The user's system prompt or the built-in one, the sub-agent addendum and the fold's header
    SystemPrompt,
    /// The definitions of the tools the model is sent
    Tools,
    KeyFacts,
    /// The user's own messages from before the fold, kept word for word
    Pinned,
    Summary,
    Notes,
    /// What the user said since the fold, the backend's notices included
    UserMessages,
    /// The model's replies since the fold, without their thinking or tool calls
    Replies,
    Thinking,
    ToolCalls,
    ToolResults,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ContextPartOut {
    kind: ContextPartKindOut,
    tokens: u64,
    chars: usize,
    /// How many there are of it (tools, facts, messages, calls); absent for one block of text
    count: Option<usize>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ChatMemoryOut {
    /// Set by the summarizer at the first fold; not editable
    goal: Option<String>,
    facts: Vec<String>,
    summary: Option<String>,
    /// The notes the model is sent; when it has written newer ones that wait for the next fold, those
    notes: Option<String>,
    /// Whether `notes` are newer than what the model is sent (they go into the prompt at the next fold)
    notes_pending: bool,
    /// Whether the chat has been folded: before that, there are no facts or summary to edit
    folded: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ChatContextResponse {
    /// The context window the chat's model runs under, in tokens
    context_length: u64,
    /// The prompt size at which the chat is folded
    fold_at: u64,
    /// `true`: `used` is the model server's own count of the last prompt, shared out over the parts
    /// by their length. `false`: nothing measured the prompt yet (a new chat, or just after a fold)
    /// and every number is an estimate from characters.
    measured: bool,
    used: u64,
    /// In the order they are sent
    parts: Vec<ContextPartOut>,
    /// Tool results that go out as a one-line stub to save room, and about how many tokens that saves
    cleared_results: usize,
    cleared_tokens: u64,
    /// Every message of the chat, and how many of them the summary replaced
    messages: u64,
    folded_messages: u64,
    /// Tokens the model's replies in this chat have cost
    generated_tokens: i64,
    subagent_chats: u64,
    memory: ChatMemoryOut,
}

/// What fills a chat's context: the prompt its next turn would send, rebuilt the same way a turn
/// builds it and measured part by part, and what the chat remembers past a fold (goal, key facts,
/// summary, notes).
#[utoipa::path(
    get,
    path = "/api/chats/context",
    tag = "chats",
    params(ChatContextQuery),
    responses(
        (status = 200, description = "The chat's context", body = ChatContextResponse),
        (status = 404, description = "No such chat", body = crate::services::error::ErrorBody),
    ),
)]
pub async fn chat_context(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<ChatContextQuery>,
) -> Result<Json<ChatContextResponse>, ErrorService> {
    let services = state.services().await?;
    let chat = services.chat_store.owned_chat(auth.id, query.chat_id).await?;
    let context = services.agent.context(query.chat_id).await?;
    let key_facts = chat.key_facts.unwrap_or_default();

    Ok(Json(ChatContextResponse {
        context_length: context.context_length,
        fold_at: context.fold_at,
        measured: context.measured,
        used: context.used,
        parts: context.parts.into_iter().map(part_out).collect(),
        cleared_results: context.cleared_results,
        cleared_tokens: context.cleared_tokens,
        messages: context.counts.messages,
        folded_messages: context.counts.folded_messages,
        generated_tokens: context.counts.generated_tokens,
        subagent_chats: context.counts.subagent_chats,
        memory: ChatMemoryOut {
            goal: key_facts.goal,
            facts: key_facts.facts,
            folded: chat.summary_up_to_message_id.is_some(),
            // Only what the model is sent: a summary with no boundary isn't
            summary: chat.summary.filter(|_| chat.summary_up_to_message_id.is_some()),
            notes_pending: chat.notes_pending.is_some(),
            notes: chat.notes_pending.or(chat.notes),
        },
    }))
}

fn part_out(part: ContextPart) -> ContextPartOut {
    let kind = match part.kind {
        ContextPartKind::SystemPrompt => ContextPartKindOut::SystemPrompt,
        ContextPartKind::Tools => ContextPartKindOut::Tools,
        ContextPartKind::KeyFacts => ContextPartKindOut::KeyFacts,
        ContextPartKind::Pinned => ContextPartKindOut::Pinned,
        ContextPartKind::Summary => ContextPartKindOut::Summary,
        ContextPartKind::Notes => ContextPartKindOut::Notes,
        ContextPartKind::UserMessages => ContextPartKindOut::UserMessages,
        ContextPartKind::Replies => ContextPartKindOut::Replies,
        ContextPartKind::Thinking => ContextPartKindOut::Thinking,
        ContextPartKind::ToolCalls => ContextPartKindOut::ToolCalls,
        ContextPartKind::ToolResults => ContextPartKindOut::ToolResults,
    };
    ContextPartOut { kind, tokens: part.tokens, chars: part.chars, count: part.count }
}
