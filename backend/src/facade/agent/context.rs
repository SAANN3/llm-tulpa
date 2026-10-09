//! What fills a chat's context: the prompt its next turn would send, rebuilt the way a turn builds it
//! (`History::parts`, `Turn::tools_for`), measured part by part. The model server reports one number
//! for the whole prompt, so the parts are measured in characters and shared out of that number in
//! proportion; before the first measurement (a new chat, or right after a fold) the total is an estimate.

use std::sync::Arc;

use super::compaction::Compaction;
use super::history::History;
use super::model_call::ModelCall;
use super::turn::Turn;
use crate::services::chat_store::{ChatCounts, ChatStore};
use crate::services::error::ErrorService;
use crate::services::llm::{estimated_prompt_tokens, tool_definitions, ChatMessage};
use crate::services::tools::ToolService;
use crate::tools::base::Tool;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContextPartKind {
    /// The user's system prompt or the built-in one, the sub-agent addendum and the fold's header
    SystemPrompt,
    Tools,
    KeyFacts,
    /// The user's own messages from before the fold, kept word for word
    Pinned,
    Summary,
    Notes,
    /// What the user said since the fold (and the backend's notices, which go out as the user's)
    UserMessages,
    /// The model's replies since the fold, without their thinking or tool calls
    Replies,
    Thinking,
    ToolCalls,
    ToolResults,
}

pub struct ContextPart {
    pub kind: ContextPartKind,
    pub chars: usize,
    pub tokens: u64,
    /// How many there are of it (tools, facts, messages, calls); `None` for one block of text
    pub count: Option<usize>,
}

pub struct ContextBreakdown {
    pub context_length: u64,
    /// The prompt size at which the chat is folded
    pub fold_at: u64,
    /// Whether `used` is the model server's own count of the last prompt (else an estimate from characters)
    pub measured: bool,
    pub used: u64,
    pub parts: Vec<ContextPart>,
    /// Tool results that go out as a one-line stub, and what the stubs leave out, in tokens
    pub cleared_results: usize,
    pub cleared_tokens: u64,
    pub counts: ChatCounts,
}

#[derive(Clone)]
pub(super) struct ContextReader {
    chat_store: Arc<ChatStore>,
    tools: Arc<ToolService>,
    history: History,
    model: ModelCall,
}

impl ContextReader {
    pub(super) fn new(chat_store: Arc<ChatStore>, tools: Arc<ToolService>, history: History, model: ModelCall) -> Self {
        Self { chat_store, tools, history, model }
    }

    pub(super) async fn breakdown(&self, chat_id: i64) -> Result<ContextBreakdown, ErrorService> {
        let (mut chat, built) = self.history.parts(chat_id).await?;
        // The model a turn in progress runs on, as `Turn::prepare` sees it
        self.model.bind(&mut chat);
        let tools = Turn::tools_for(&chat, self.tools.snapshot_tools().await);
        let tool_refs: Vec<&dyn Tool> = tools.iter().map(|t| t.as_ref()).collect();
        let tools_chars = if tool_refs.is_empty() { 0 } else { serde_json::to_string(&tool_definitions(&tool_refs)).unwrap_or_default().len() };

        let mut system_prompt = self.history.base_system_prompt(&chat).await?.len();
        let mut parts = Vec::new();
        if let Some(fold) = &built.fold {
            system_prompt += fold.header.len();
            let facts = chat.key_facts.as_ref().map(|k| k.facts.len() + usize::from(k.goal.is_some()));
            parts.push(Self::part(ContextPartKind::KeyFacts, fold.key_facts.as_ref().map_or(0, String::len), facts));
            parts.push(Self::part(ContextPartKind::Pinned, fold.pinned.as_ref().map_or(0, String::len), None));
            parts.push(Self::part(ContextPartKind::Summary, fold.summary.len(), None));
        }
        parts.insert(0, Self::part(ContextPartKind::Tools, tools_chars, Some(tools.len())));
        parts.insert(0, Self::part(ContextPartKind::SystemPrompt, system_prompt, None));
        parts.push(Self::part(ContextPartKind::Notes, built.notes.as_ref().map_or(0, String::len), None));
        parts.extend(Self::message_parts(&built.messages));

        let total_chars: usize = parts.iter().map(|p| p.chars).sum();
        let measured = chat.last_prompt_tokens.filter(|t| *t > 0).map(|t| t as u64);
        // Measured: the one number the server reported, shared out by characters. Estimated: each part on its own.
        let tokens_of = |chars: usize| match measured {
            Some(used) if total_chars > 0 => (chars as f64 * used as f64 / total_chars as f64).round() as u64,
            _ => estimated_prompt_tokens(chars),
        };
        for part in &mut parts {
            part.tokens = tokens_of(part.chars);
        }
        let used = measured.unwrap_or_else(|| parts.iter().map(|p| p.tokens).sum());
        let context_length = self.model.context_of(&chat).await?;
        Ok(ContextBreakdown {
            context_length,
            fold_at: Compaction::trigger_tokens(context_length),
            measured: measured.is_some(),
            used,
            parts,
            cleared_results: built.cleared_results,
            cleared_tokens: tokens_of(built.cleared_chars),
            counts: self.chat_store.chat_counts(chat_id).await?,
        })
    }

    fn part(kind: ContextPartKind, chars: usize, count: Option<usize>) -> ContextPart {
        ContextPart { kind, chars, tokens: 0, count }
    }

    /// The messages' share of each kind. A reply's thinking is replayed inside its content (`<think>…</think>`
    /// in front, see `History::to_message_capped`), so it is split off here to be counted on its own.
    fn message_parts(messages: &[ChatMessage]) -> Vec<ContextPart> {
        let mut user = Self::part(ContextPartKind::UserMessages, 0, Some(0));
        let mut replies = Self::part(ContextPartKind::Replies, 0, Some(0));
        let mut thinking = Self::part(ContextPartKind::Thinking, 0, Some(0));
        let mut calls = Self::part(ContextPartKind::ToolCalls, 0, Some(0));
        let mut results = Self::part(ContextPartKind::ToolResults, 0, Some(0));
        let add = |part: &mut ContextPart, chars: usize, n: usize| {
            part.chars += chars;
            part.count = part.count.map(|count| count + n);
        };
        for message in messages {
            match message.role.as_str() {
                "assistant" => {
                    let (thought, text) = Self::split_thinking(&message.content);
                    if thought > 0 {
                        add(&mut thinking, thought, 1);
                    }
                    if !text.trim().is_empty() {
                        add(&mut replies, text.len(), 1);
                    }
                    for call in message.tool_calls.iter().flatten() {
                        add(&mut calls, call.function.name.len() + call.function.arguments.to_string().len(), 1);
                    }
                }
                "tool" => add(&mut results, message.content.len(), 1),
                _ => add(&mut user, message.content.len(), 1),
            }
        }
        vec![user, replies, thinking, calls, results]
    }

    /// How long the replayed thinking in front of a reply is, and the reply after it.
    fn split_thinking(content: &str) -> (usize, &str) {
        const CLOSE: &str = "\n</think>\n\n";
        match content.strip_prefix("<think>\n").and_then(|rest| rest.find(CLOSE)) {
            Some(end) => {
                let thought_end = "<think>\n".len() + end + CLOSE.len();
                (thought_end, &content[thought_end..])
            }
            None => (0, content),
        }
    }
}
