//! One step of a turn: build the request, ask the model, store its reply, hand back the tool calls it
//! asked for, and keep the chat inside its window. A step does not loop; whoever drives the turn
//! (`Agent` today) runs the tools it asks for and calls the next step.

use std::sync::Arc;
use std::time::Instant;

use super::compaction::Compaction;
use super::history::History;
use super::model_call::{ModelCall, Regenerations, ReplyProblem};
use super::prompts;
use super::run_tracker::RunTracker;
use super::tool_calls::ToolCalls;
use super::{ChatOut, NoticeOut};
use crate::services::chat_store::{Chat, ChatStore, MessageTimings, NewMessage, NewToolCall};
use crate::services::error::ErrorService;
use crate::services::event_bus::{EventBus, ServerEvent};
use crate::services::llm::{CallParams, ChatMessage, ChatResponse, LlmProvider, ThinkChoice};
use crate::services::tools::ToolService;
use crate::tools::base::Tool;
use crate::tools::subagent;

/// How many of a chat's newest messages `Turn::make_room` looks through for what was added since the
/// model's last reply: tool results and notices, a handful at most.
const RECENT_MESSAGES_LOOKBACK: u64 = 500;
/// Tokens a character of content not yet sent is counted as. Lower than the measured average of a tool-heavy
/// chat (see `compaction::clearing`) on purpose: counting too much only folds a little earlier, counting too little
/// sends a request the window can't hold.
const NEW_CONTENT_CHARS_PER_TOKEN: f64 = 3.0;

/// What a step is given: the history to send (every message is already stored; the newest is the one
/// being answered), and what to do around the model call.
pub(super) struct Step {
    pub(super) chat_id: i64,
    pub(super) messages: Vec<ChatMessage>,
    pub(super) think: Option<ThinkChoice>,
    /// Job notices the caller stored just before this step, carried into its output so a client can
    /// show them ahead of the reply.
    pub(super) notices: Vec<NoticeOut>,
    /// Whether a reply cut off while still thinking may be stored and continued on its own: once per turn.
    pub(super) can_auto_continue: bool,
    /// Set on the last step the user's step limit allows (the limit itself): the model is told the turn
    /// is about to be stopped, and any tool call in its reply is refused while its text is kept. The
    /// request still carries the same tools, so the model server's cached prompt serves it.
    pub(super) step_limit: Option<u32>,
    pub(super) run: RunTracker,
}

/// What a step came to.
pub(super) enum StepOut {
    Reply(ChatOut),
    /// The run was stopped while the model call was in flight: nothing was stored.
    Stopped,
}

impl StepOut {
    /// The reply, for a caller whose run nobody can stop.
    pub(super) fn into_reply(self) -> Result<ChatOut, ErrorService> {
        match self {
            StepOut::Reply(out) => Ok(out),
            StepOut::Stopped => Err(ErrorService::internal("the turn was stopped")),
        }
    }
}

/// The request one step sends, built once and sent again if the reply has to be regenerated.
struct Request {
    provider: Arc<dyn LlmProvider>,
    params: CallParams,
    model: String,
    known_prompt_tokens: Option<u64>,
    /// The tools this chat is shown, already filtered for a sub-agent's chat.
    tools: Vec<Arc<dyn Tool>>,
    /// System message first.
    messages: Vec<ChatMessage>,
    /// Files `ui.attach_file` queued in this turn: they land on the reply that ends it.
    attached_files: Vec<i64>,
}

/// What asking the model came to.
enum Asked {
    Reply(ChatResponse),
    /// Cut off by the token limit while still thinking, and this step may continue on its own.
    CutOff(ChatResponse),
    Stopped,
}

#[derive(Clone)]
pub(super) struct Turn {
    chat_store: Arc<ChatStore>,
    tools: Arc<ToolService>,
    events: Arc<EventBus>,
    history: History,
    model: ModelCall,
    compaction: Compaction,
    tool_calls: ToolCalls,
}

impl Turn {
    pub(super) fn new(
        chat_store: Arc<ChatStore>,
        tools: Arc<ToolService>,
        events: Arc<EventBus>,
        history: History,
        model: ModelCall,
        compaction: Compaction,
        tool_calls: ToolCalls,
    ) -> Self {
        Self { chat_store, tools, events, history, model, compaction, tool_calls }
    }

    /// Compacts the chat first when the request about to be sent would not fit. The size is the one the
    /// model server measured for the last request plus what has been added since: a tool result can be
    /// bigger than everything before it, and a check made on the last measurement alone lets the next
    /// request through to a 400 from the server that no retry fixes. `extra_chars` is what the caller is
    /// about to add and hasn't stored yet (the user's prompt).
    pub(super) async fn make_room(&self, chat_id: i64, think: Option<ThinkChoice>, extra_chars: usize) {
        let estimated = self.estimated_prompt_tokens(chat_id, extra_chars).await;
        self.compaction.maybe_compact(chat_id, estimated, think).await;
    }

    /// `None` when the chat has no measurement yet (nothing has been sent), or can't be read: a failed
    /// lookup is a reason to skip the check, not to fail the step.
    async fn estimated_prompt_tokens(&self, chat_id: i64, extra_chars: usize) -> Option<u64> {
        let known = self.chat_store.chat(chat_id).await.ok()?.last_prompt_tokens? as u64;
        // Newest first: everything after the model's last reply is what the measurement doesn't include
        let added_chars: usize = match self.chat_store.messages(chat_id, RECENT_MESSAGES_LOOKBACK, 0).await {
            Ok((recent, _)) => recent.iter().take_while(|m| m.role != "assistant").map(|m| m.content.len()).sum(),
            Err(_) => 0,
        };
        Some(Self::with_added(known, added_chars + extra_chars))
    }

    /// The last measured prompt size plus `added_chars` of content that measurement doesn't include.
    fn with_added(known_tokens: u64, added_chars: usize) -> u64 {
        known_tokens + (added_chars as f64 / NEW_CONTENT_CHARS_PER_TOKEN) as u64
    }

    /// Runs one step. A reply that asks for tools means the turn goes on (the tools run, the model is
    /// called again): the model server stays claimed. Anything else ends it.
    pub(super) async fn step(&self, step: Step) -> Result<StepOut, ErrorService> {
        let chat_id = step.chat_id;
        let result = self.step_once(step).await;
        if !matches!(&result, Ok(StepOut::Reply(out)) if out.can_use_tools) {
            self.model.release(chat_id);
        }
        result
    }

    async fn step_once(&self, step: Step) -> Result<StepOut, ErrorService> {
        let request = self.prepare(&step).await?;
        // Times the whole model call, regenerations included: that is how long the reply really took
        let started_at = Instant::now();
        let asked = self.ask(&step, &request).await?;
        let thought_duration_ms = i64::try_from(started_at.elapsed().as_millis()).unwrap_or(i64::MAX);
        match asked {
            Asked::Stopped => Ok(StepOut::Stopped),
            Asked::CutOff(response) => self.continue_after_cut_off(step, response, thought_duration_ms).await,
            Asked::Reply(response) => {
                self.store_reply(step, request.attached_files, response, thought_duration_ms).await.map(StepOut::Reply)
            }
        }
    }

    /// The request: the chat's model, provider, tools and call parameters, and the messages with the
    /// system message in front.
    async fn prepare(&self, step: &Step) -> Result<Request, ErrorService> {
        let chat_id = step.chat_id;
        let mut messages = step.messages.clone();

        // Snapshot the current tool set once per step. Each element is an Arc<dyn Tool>
        // that can be held past any .await without keeping the ToolService lock open,
        // so plugin enable/disable can update the set concurrently with ongoing turns.
        let tools_snapshot: Vec<Arc<dyn Tool>> = self.tools.snapshot_tools().await;

        // The chat metadata, read fresh so a model switch takes effect with the next prompt and
        // last_prompt_tokens is available for accurate budget calculation.
        let mut chat: Chat = self.chat_store.chat(chat_id).await?;
        self.model.bind(&mut chat);

        // Which tools the model is shown depends on whether this chat is a sub-agent's — see
        // `subagent::available_to`. Filtering the list (rather than only refusing a call) is what
        // keeps a sub-agent from being able to try starting one of its own.
        let is_subagent = chat.parent_chat_id.is_some();
        let tools: Vec<Arc<dyn Tool>> = tools_snapshot
            .into_iter()
            .filter(|t| subagent::available_to(t.function_name(), is_subagent))
            .collect();
        // A chat without tools sends none: the definitions are most of what a small window has to spare
        let tools = if chat.tools_enabled { tools } else { Vec::new() };
        let provider = self.model.provider(&chat)?;
        let params = self.model.params(&chat).await?;
        self.model.hold(&chat, provider.as_ref(), params.launch.as_ref()).await?;

        // `History::for_chat` leads with its own system message (the compaction summary)
        // once a chat has one — folded into this same system message rather than sent
        // as a second one, since some chat templates (e.g. Qwen's) reject more than one
        // system-role message anywhere but position 0 ("System message must be at the
        // beginning"). Deliberately holds no per-call-volatile content (no timestamp —
        // see `now_note` below for why) so it stays byte-identical across a chat's
        // turns except when a compaction fold actually changes the summary — that's
        // what lets Ollama/llama.cpp's prompt cache match this prefix and reuse it
        // instead of reprocessing the whole history on every single turn.
        // The user's own system prompt, if they set one — fetched once per turn, the same
        // as the chat row above, so a change (or reset) takes effect from the next turn,
        // exactly like a model switch. A user without one gets the built-in default.
        let system_prompt = self.history.system_prompt(&chat, &mut messages).await?;

        // Collected now, while `messages` still holds this turn's history, so it
        // survives the `extend` below. Only actually used once we know this response
        // has no further tool calls of its own — see `store_reply`.
        let attached_files = History::attached_files(&messages);
        let streak_note = History::streak_notice(&messages);

        let mut messages_with_system = vec![ChatMessage::system(system_prompt)];
        messages_with_system.extend(messages);

        // Stamped onto the newest message (the user's prompt, or the tool result just stored).
        // That's content this request sends to the model for the first time, so appending it
        // here doesn't cost any additional
        // prompt-cache reuse — unlike baking it into `system_prompt` above (the old
        // approach), which changed every single call and, being the prompt's very
        // first tokens, invalidated the *entire* cached prefix on every turn (see
        // `SYSTEM_PROMPT`'s own rule for the paired instruction — it now points here
        // instead of claiming a fixed position). Computed fresh each call so it's
        // never stale, same as before; still reaches every `Agent` instance including
        // the messaging-plugin one (empty `ToolService`, no `os.get_date` to fall back
        // on).
        let now_note = format!(
            "\n\n[Current real date and time (UTC): {}]",
            chrono::Utc::now().format("%A, %B %-d, %Y %H:%M:%S UTC")
        );
        // Like the date, the step-limit warning goes on the newest message only: the prompt in front of it
        // is the one the server already holds.
        let limit_note = step.step_limit.map(prompts::step_limit_note);
        let notes = [Some(now_note), streak_note, limit_note];
        if let Some(last) = messages_with_system.last_mut() {
            notes.iter().flatten().for_each(|note| last.content.push_str(note));
        }

        Ok(Request {
            provider,
            params,
            model: chat.model.clone(),
            known_prompt_tokens: chat.last_prompt_tokens.map(|t| t as u64),
            tools,
            messages: messages_with_system,
            attached_files,
        })
    }

    /// Sends the request and applies the reply policy: an unusable reply is asked for again (up to the
    /// limit of its kind), one cut off in its thinking is handed back for `continue_after_cut_off` when
    /// the step may continue on its own, and anything else is kept.
    async fn ask(&self, step: &Step, request: &Request) -> Result<Asked, ErrorService> {
        let chat_id = step.chat_id;
        let tools: Vec<&dyn Tool> = request.tools.iter().map(|t| t.as_ref()).collect();
        let mut regenerations = Regenerations::default();
        loop {
            step.run.call_started();
            let call = request.provider.chat(
                request.messages.clone(),
                None,
                &tools,
                step.think.clone(),
                &request.model,
                request.known_prompt_tokens,
                &request.params,
            );
            // A stop drops the call mid-flight: the request is closed and the server stops generating
            let response = tokio::select! {
                _ = step.run.stopped() => {
                    step.run.call_abandoned();
                    return Ok(Asked::Stopped);
                }
                response = call => response?,
            };
            step.run.call_finished(response.eval_count(), response.prompt_eval_count());

            // The live "thinking" indicator's spend hint (see `ServerEvent::TurnProgress`):
            // model calls are non-streaming, so a token count exists only at the moment one
            // returns — publish it there, including for regenerated (unusable) replies,
            // since their tokens were spent too.
            if let Some(eval_tokens) = response.eval_count() {
                self.events.publish(ServerEvent::TurnProgress {
                    chat_id,
                    eval_tokens,
                    prompt_tokens: response.prompt_eval_count(),
                    step: step.run.snapshot().step,
                });
            }

            let Some(problem) = ModelCall::unusable_reply(request.provider.as_ref(), &response, &request.model).await else {
                return Ok(Asked::Reply(response));
            };

            if problem == ReplyProblem::CutOffInThinking && step.can_auto_continue {
                return Ok(Asked::CutOff(response));
            }

            if !regenerations.allow(problem) {
                tracing::warn!(chat_id, problem = problem.describe(), "model reply unusable, keeping it anyway");
                return Ok(Asked::Reply(response));
            }

            let thinking_tail: String = response
                .message
                .thinking
                .as_deref()
                .unwrap_or_default()
                .chars()
                .rev()
                .take(160)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            tracing::warn!(chat_id, problem = problem.describe(), %thinking_tail, "model reply unusable, regenerating");
        }
    }

    /// A reply that ran out of tokens while still thinking: its thoughts are stored as a message, a
    /// notice asks the model to carry on, and the step runs again (once: the next one may not
    /// continue on its own).
    async fn continue_after_cut_off(
        &self,
        step: Step,
        response: ChatResponse,
        thought_duration_ms: i64,
    ) -> Result<StepOut, ErrorService> {
        let chat_id = step.chat_id;
        let thought_trace = response
            .message
            .thinking
            .as_deref()
            .unwrap_or(&response.message.content)
            .trim()
            .to_string();
        let prompt_eval_count = response.prompt_eval_count();
        let eval_count = response.eval_count();
        let timings = ModelCall::timings_of(&response);

        tracing::warn!(
            chat_id,
            "model cut off in thinking; storing thought process as message and triggering automatic continuation"
        );

        // Store cut thoughts as message content (not as thinking) with an explicit continuation
        // marker so the model on the continuation turn knows it was interrupted mid-thought.
        let content = prompts::cut_off_thoughts_message(&thought_trace);
        self.chat_store
            .new_message(NewMessage {
                chat_id,
                role: "assistant".to_string(),
                content,
                tool_name: None,
                thinking: None,
                thought_duration_ms: Some(thought_duration_ms),
                tool_success: None,
                tool_denied: false,
                tool_calls: vec![],
                images: vec![],
                file_ids: vec![],
                prompt_tokens: prompt_eval_count.map(|c| c as i64),
                eval_tokens: eval_count.map(|c| c as i64),
                timings,
            })
            .await?;

        let total_tokens = prompt_eval_count.map(|pt| pt + eval_count.unwrap_or(0));
        if let Some(total) = total_tokens {
            if let Err(e) = self.chat_store.set_last_prompt_tokens(chat_id, Some(total as i64)).await {
                tracing::warn!(chat_id, "failed to update last_prompt_tokens: {e:?}");
            }
        }

        // Check compaction against total tokens (prompt + generated thoughts), not just prompt_eval_count,
        // so compaction frees headroom BEFORE the recursive continuation turn if context is full.
        self.compaction.maybe_compact(chat_id, total_tokens, step.think.clone()).await;

        // Persist the continuation prompt into the chat store so the database message
        // sequence is strictly alternating (assistant -> notice -> assistant),
        // preventing Ollama's "Cannot have 2 or more assistant messages at the end" error.
        // `notice` (not `user`) so this renders as the same muted, backend-written marker a
        // finished-job notice does, not a fake chat bubble the user never actually typed —
        // `History::to_message` already sends any `notice` to Ollama as a `user` turn either way.
        let continuation_text = prompts::CUT_OFF_CONTINUATION.to_string();
        self.chat_store
            .new_message(NewMessage {
                chat_id,
                role: "notice".to_string(),
                content: continuation_text,
                tool_name: None,
                thinking: None,
                thought_duration_ms: None,
                tool_success: None,
                tool_denied: false,
                tool_calls: vec![],
                images: vec![],
                file_ids: vec![],
                prompt_tokens: None,
                eval_tokens: None,
                timings: MessageTimings::default(),
            })
            .await?;

        let fresh_messages = self.history.for_chat(chat_id).await?;
        let next = Step {
            chat_id,
            messages: fresh_messages,
            think: step.think,
            notices: step.notices,
            can_auto_continue: false,
            step_limit: step.step_limit,
            run: step.run,
        };
        Box::pin(self.step(next)).await
    }

    /// Stores the reply (with the tool calls it asked for), records how big the prompt now is, and
    /// compacts when that crossed the trigger.
    async fn store_reply(
        &self,
        step: Step,
        attached_files: Vec<i64>,
        response: ChatResponse,
        thought_duration_ms: i64,
    ) -> Result<ChatOut, ErrorService> {
        let chat_id = step.chat_id;
        let prompt_eval_count = response.prompt_eval_count();
        let eval_count = response.eval_count();
        let prompt_tokens = prompt_eval_count.map(|c| c as i64);
        let eval_tokens = eval_count.map(|c| c as i64);
        let timings = ModelCall::timings_of(&response);

        let thinking = response.message.thinking.clone();

        let mut requested_tool_calls = response.message.tool_calls.unwrap_or_default();
        if step.step_limit.is_some() && !requested_tool_calls.is_empty() {
            // The model was told this reply is text only; what it wrote is kept, what it tried to call is not
            tracing::warn!(chat_id, calls = requested_tool_calls.len(), "tool calls in the reply at the step limit refused");
            requested_tool_calls.clear();
        }
        let new_tool_calls: Vec<NewToolCall> = requested_tool_calls
            .iter()
            .map(|call| NewToolCall {
                tool_name: call.function.name.clone(),
                arguments: call.function.arguments.clone(),
            })
            .collect();

        // `ui.attach_file` results only ever land on the turn's *final* reply — the
        // message the model actually prints once it's done calling tools — never on
        // an intermediate tool-calling message, which usually has no real content of
        // its own for a file to visibly hang off of.
        let file_ids = if new_tool_calls.is_empty() { attached_files } else { vec![] };

        let stored = self
            .chat_store
            .new_message(NewMessage {
                chat_id,
                role: response.message.role,
                content: response.message.content,
                tool_name: None,
                thinking: thinking.clone(),
                thought_duration_ms: Some(thought_duration_ms),
                tool_success: None,
                tool_denied: false,
                tool_calls: new_tool_calls,
                images: vec![],
                file_ids,
                prompt_tokens,
                eval_tokens,
                timings,
            })
            .await?;

        if let Some(pt) = prompt_eval_count {
            let total = pt + eval_count.unwrap_or(0);
            if let Err(e) = self.chat_store.set_last_prompt_tokens(chat_id, Some(total as i64)).await {
                tracing::warn!(chat_id, "failed to update last_prompt_tokens: {e:?}");
            }
        }

        let mut tool_calls = Vec::with_capacity(requested_tool_calls.len());
        for call in requested_tool_calls {
            tool_calls.push(
                self.tool_calls
                    .to_agent_tool_call(chat_id, call.function.name, call.function.arguments)
                    .await?,
            );
        }

        let out = ChatOut {
            id: stored.id,
            content: stored.content,
            created_at: stored.created_at,
            can_use_tools: !tool_calls.is_empty(),
            tool_calls,
            thinking,
            thought_duration_ms,
            file_ids: stored.file_ids,
            notices: step.notices,
            eval_tokens,
            prompt_tokens,
            user_message_id: None,
        };

        let total_tokens = prompt_eval_count.map(|pt| pt + eval_count.unwrap_or(0));
        self.compaction.maybe_compact(chat_id, total_tokens, step.think).await;

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_added_since_the_last_measurement_counts_toward_the_next_request() {
        assert_eq!(Turn::with_added(10_000, 0), 10_000);
        // 90,000 characters (a large tool result) is ~30,000 tokens at the cautious ratio
        assert_eq!(Turn::with_added(10_000, 90_000), 40_000);
    }
}
