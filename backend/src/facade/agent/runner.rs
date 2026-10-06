//! The backend's loop of a turn: one run per chat that asks the model, runs the tools it asks for,
//! and asks again until the model answers, the user stops it, a step limit is reached, or a tool
//! needs the user's permission. The browser only starts a run, answers a permission prompt and
//! watches (events and `state`); a closed tab doesn't stop anything.
//!
//! Nothing here waits for a person: a tool call that needs permission ends the run in the state
//! *waiting for permission* (read back from the stored chat), and the answer starts a new run.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use sea_orm::prelude::DateTimeUtc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use super::notices::Notices;
use super::prompts;
use super::run_tracker::{RunSnapshot, RunTracker};
use super::tool_calls::ToolCalls;
use super::turn::{Step, StepOut, Turn};
use super::history::History;
use super::{AgentToolCall, AgentToolPermission, ChatOut};
use crate::services::chat_store::{ChatStore, MessageTimings, NewMessage};
use crate::services::error::ErrorService;
use crate::services::event_bus::{EventBus, RunEndReason, ServerEvent};
use crate::services::file_store::FileStore;
use crate::services::llm::ThinkChoice;
use crate::services::settings_store::SettingsStore;
use crate::tools::llm::return_agent::ReturnAgentTool;

/// Model calls a sub-agent gets before its run is cut off. A sub-agent that keeps calling tools
/// without ever answering would otherwise run for as long as the model keeps going, holding the
/// only GPU while the chat that delegated to it waits.
const SUBAGENT_MAX_MODEL_CALLS: u32 = 100;

/// How a user answers a permission prompt for one pending tool call.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Allowance {
    /// Grant what the call asked for, now and for later calls in this chat.
    Permanent,
    /// Let this one call through with what it asked for.
    OnlyNow,
    /// Refuse it: the model is told the call was denied.
    Deny,
}

/// The answer to one pending tool call: `index` is its position in `TurnState::pending`.
#[derive(Clone, Copy, Debug, Deserialize, ToSchema)]
pub struct Decision {
    pub index: usize,
    pub allowance: Allowance,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatus {
    Idle,
    Running,
    /// A tool call needs the user's permission; see `TurnState::pending`.
    WaitingForPermission,
}

/// How the last run on a chat ended, kept until the next one starts so a page that was closed meanwhile
/// can still say why nothing is going on.
#[derive(Clone, Serialize, ToSchema)]
pub struct RunEnded {
    pub reason: RunEndReason,
    /// What failed, for `failed`.
    pub detail: Option<String>,
    #[schema(value_type = String, format = "date-time")]
    pub ended_at: DateTimeUtc,
    /// When the run started and the tokens its model calls generated: "stopped after 2m 10s, 3.4k tokens".
    #[schema(value_type = String, format = "date-time")]
    pub started_at: DateTimeUtc,
    pub eval_tokens: u64,
    /// The HTTP status a `failed` run would have had: 423 means the model server is in use by someone else.
    pub status: Option<u16>,
}

/// What a chat's turn is doing, for a page that was just opened or reloaded.
#[derive(Serialize, ToSchema)]
pub struct TurnState {
    pub status: TurnStatus,
    /// When the run started (while running).
    #[schema(value_type = Option<String>, format = "date-time")]
    pub started_at: Option<DateTimeUtc>,
    /// When the model call in flight started; `None` while a tool runs or between calls.
    #[schema(value_type = Option<String>, format = "date-time")]
    pub call_started_at: Option<DateTimeUtc>,
    /// Tokens the run's finished model calls generated so far.
    pub eval_tokens: u64,
    /// The prompt size the run's last finished model call measured.
    pub prompt_tokens: Option<u64>,
    /// The step the run is on (1 for the first model call) and the user's step limit, when one is set.
    pub step: u32,
    pub step_limit: Option<u32>,
    /// The tool call that is running and since when; both `None` while a model call is in flight.
    pub running_tool: Option<String>,
    #[schema(value_type = Option<String>, format = "date-time")]
    pub tool_started_at: Option<DateTimeUtc>,
    /// While waiting for permission: the tool calls the model asked for that are not run yet, in
    /// order, with what each needs. `Decision::index` points into this list.
    pub pending: Vec<AgentToolCall>,
    pub last_end: Option<RunEnded>,
}

/// What `start_prompt` returns at once: the run goes on in the background.
pub struct StartedTurn {
    pub user_message_id: i64,
}

/// Who a run is for, which decides what happens at a tool call that needs permission.
#[derive(Clone, Copy)]
pub(super) enum Policy {
    /// A user's chat: the run ends and waits for the user's answer, unless they have auto-confirm on.
    Attended,
    /// A sub-agent's chat: nobody can be asked, so what the escalation offers is granted with auto-confirm
    /// and the call is refused without it. The run also ends at `llm.return_agent`.
    Subagent,
}

/// What a run starts with.
enum Start {
    /// The user's prompt, already stored.
    Prompt,
    /// A finished background job (or sub-agent): its notice is the reason for the run.
    Wake,
    /// The model answers again where it answered last, and the old reply (`replace`) goes once the
    /// new one is stored.
    Regenerate { replace: i64 },
    /// The user's answers to the pending tool calls; they run, then the model is asked again.
    Answer { decisions: Vec<Decision> },
    /// A sub-agent's run continues from what its chat holds.
    Subagent,
}

/// How a run ended.
enum RunEnd {
    Answered,
    StepLimit,
    Stopped,
    WaitingForPermission,
    Failed(ErrorService),
    /// A wake-up with nothing to report: no run took place.
    NothingToDo,
    /// A sub-agent's result (see `SubagentResult`).
    Subagent(SubagentResult),
}

/// How a sub-agent's run ended.
pub(super) enum SubagentResult {
    /// The text it handed back with `llm.return_agent`, or, when it just stopped calling tools,
    /// its last message.
    Answer(String),
    /// It didn't reach an answer (it ran out of model calls, or stopped with nothing to say). The
    /// text says why and carries whatever it had written last, if anything.
    Incomplete(String),
}

/// A sub-agent's attempt to hand its result back that the tool refused (a missing `output`, say).
struct FailedReturn {
    /// What the tool said.
    error: String,
    /// What the sub-agent wrote alongside the call — usually the answer it meant to return.
    attempt: String,
}

/// What running the pending tool calls came to.
enum ToolsOutcome {
    /// Every pending call ran. `returned` is the sub-agent's `llm.return_agent` outcome, if it called it.
    Done { returned: Option<Returned> },
    NeedsPermission,
    Stopped,
}

enum Returned {
    Answer(String),
    Refused(String),
}

/// The claim a run has on its chat until it ends. Dropping it frees the chat — on any exit, a panic included.
struct RunSlot {
    runs: Arc<Mutex<HashMap<i64, RunTracker>>>,
    chat_id: i64,
    run: RunTracker,
}

impl Drop for RunSlot {
    fn drop(&mut self) {
        self.runs.lock().unwrap().remove(&self.chat_id);
    }
}

#[derive(Clone)]
pub(super) struct TurnRunner {
    chat_store: Arc<ChatStore>,
    settings_store: Arc<SettingsStore>,
    file_store: Arc<FileStore>,
    events: Arc<EventBus>,
    history: History,
    notices: Notices,
    tool_calls: ToolCalls,
    turn: Turn,
    /// The chats with a run going on: one at a time per chat.
    runs: Arc<Mutex<HashMap<i64, RunTracker>>>,
    /// The `think` choice each chat's last run started with: a run the backend starts itself (a
    /// finished job) uses the one the user last chose.
    thinks: Arc<Mutex<HashMap<i64, Option<ThinkChoice>>>>,
    last_ends: Arc<Mutex<HashMap<i64, RunEnded>>>,
}

impl TurnRunner {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        chat_store: Arc<ChatStore>,
        settings_store: Arc<SettingsStore>,
        file_store: Arc<FileStore>,
        events: Arc<EventBus>,
        history: History,
        notices: Notices,
        tool_calls: ToolCalls,
        turn: Turn,
    ) -> Self {
        Self {
            chat_store,
            settings_store,
            file_store,
            events,
            history,
            notices,
            tool_calls,
            turn,
            runs: Arc::new(Mutex::new(HashMap::new())),
            thinks: Arc::new(Mutex::new(HashMap::new())),
            last_ends: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // ---- starting, stopping, reading ----

    /// Stores the user's prompt and starts a run that answers it. Refused (409) while the chat has a
    /// run going: two tabs on one chat must not run the same turn twice.
    pub(super) async fn start_prompt(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        file_ids: Vec<i64>,
        think: Option<ThinkChoice>,
    ) -> Result<StartedTurn, ErrorService> {
        let slot = self.claim(chat_id)?;
        // A call a crash cut short gets its result before this message, so the history stays in order
        self.tool_calls.settle_interrupted(chat_id).await?;

        // A file can be uploaded before any chat exists to attach it to (the home page's case): this is the
        // moment it becomes this chat's. A no-op for a file already uploaded with a real `chat_id`.
        for &file_id in &file_ids {
            self.file_store.attach_to_chat(file_id, chat_id).await?;
        }
        let stored = self.store_user_message(chat_id, prompt, images, file_ids).await?;

        self.spawn(slot, think, Start::Prompt, Policy::Attended);
        Ok(StartedTurn { user_message_id: stored })
    }

    /// Has the model answer again where it answered last, and replaces that answer with the new one. Only a
    /// plain final reply qualifies: the chat's newest message, from the assistant, with no tool calls,
    /// straight after the user's own message (so no tool ran in between, which would be run again or
    /// answered differently) and newer than the compaction boundary (an older one is no longer part of the
    /// history the model is sent), in an ordinary chat — not a sub-agent's and not a messaging plugin's.
    /// `message_id` is the reply the caller is looking at, so a stale view of the chat can't replace a
    /// different message.
    ///
    /// The old reply is deleted only after the new one is stored: a model that is down or fails leaves the
    /// chat as it was. The history sent is the stored one minus that reply, and job notices are left for
    /// the next step to flush — one flushed now would land between the old and the new reply.
    pub(super) async fn start_regenerate(&self, chat_id: i64, message_id: i64, think: Option<ThinkChoice>) -> Result<(), ErrorService> {
        let slot = self.claim(chat_id)?;
        self.check_regenerable(chat_id, message_id).await?;
        self.spawn(slot, think, Start::Regenerate { replace: message_id }, Policy::Attended);
        Ok(())
    }

    async fn check_regenerable(&self, chat_id: i64, message_id: i64) -> Result<(), ErrorService> {
        let not_regenerable = |why: &str| ErrorService::new(StatusCode::CONFLICT, format!("that reply can't be regenerated: {why}"));

        let chat = self.chat_store.chat(chat_id).await?;
        // A sub-agent's chat is driven by the backend, and a plugin's reply has already gone out to
        // the messaging app, where a replacement here would never arrive.
        if chat.parent_chat_id.is_some() {
            return Err(not_regenerable("a sub-agent's chat runs on its own"));
        }
        if self.chat_store.is_plugin_chat(chat_id).await? {
            return Err(not_regenerable("its reply has already been sent to a messaging app"));
        }
        let (newest, _) = self.chat_store.messages(chat_id, 2, 0).await?;
        let [reply, before] = newest.as_slice() else {
            return Err(not_regenerable("it isn't a reply to a message"));
        };
        if reply.id != message_id {
            return Err(not_regenerable("it isn't the newest message"));
        }
        if reply.role != "assistant" || !reply.tool_calls.is_empty() {
            return Err(not_regenerable("only a plain reply can be"));
        }
        if before.role != "user" {
            return Err(not_regenerable("it doesn't directly follow a message of yours"));
        }
        if chat.summary_up_to_message_id.is_some_and(|boundary| reply.id <= boundary) {
            return Err(not_regenerable("it is already folded into the chat's summary"));
        }
        Ok(())
    }

    /// Starts a run from the user's answers to the permission prompt the chat is waiting at.
    pub(super) async fn start_answer(
        &self,
        chat_id: i64,
        decisions: Vec<Decision>,
        think: Option<ThinkChoice>,
    ) -> Result<(), ErrorService> {
        let slot = self.claim(chat_id)?;
        let pending = self.tool_calls.can_use(chat_id).await?.tools;
        if pending.is_empty() {
            return Err(ErrorService::new(StatusCode::CONFLICT, "the chat has no tool call waiting for an answer"));
        }
        for decision in &decisions {
            let call = pending.get(decision.index).ok_or_else(|| {
                ErrorService::new(StatusCode::BAD_REQUEST, format!("there is no pending tool call number {}", decision.index))
            })?;
            let can_grant = matches!(call.permission, AgentToolPermission::Denied { escalation: Some(_), .. });
            if decision.allowance != Allowance::Deny && !can_grant {
                return Err(ErrorService::new(
                    StatusCode::BAD_REQUEST,
                    format!("pending tool call number {} offers nothing that could be granted", decision.index),
                ));
            }
        }
        self.spawn(slot, think, Start::Answer { decisions }, Policy::Attended);
        Ok(())
    }

    /// Starts a run because a background job (or sub-agent) of the chat finished, when nothing else is
    /// going on in it: no run, and no tool call waiting for permission (a new message would supersede it, so
    /// its notice waits for the user's answer instead).
    pub(super) async fn wake(&self, chat_id: i64) -> Result<(), ErrorService> {
        let chat = self.chat_store.chat(chat_id).await?;
        // A sub-agent's chat is driven by its own run, and a messaging plugin's reply would reach nobody
        if chat.parent_chat_id.is_some() || self.chat_store.is_plugin_chat(chat_id).await? {
            return Ok(());
        }
        let Ok(slot) = self.claim(chat_id) else { return Ok(()) };
        if self.waiting_for_permission(chat_id).await? {
            return Ok(());
        }
        self.tool_calls.settle_interrupted(chat_id).await?;
        let think = self.thinks.lock().unwrap().get(&chat_id).cloned().flatten();
        self.spawn(slot, think, Start::Wake, Policy::Attended);
        Ok(())
    }

    /// Whether the chat has a run going on.
    pub(super) fn has_run(&self, chat_id: i64) -> bool {
        self.runs.lock().unwrap().contains_key(&chat_id)
    }

    /// Stops the chat's run: the model call in flight is dropped and nothing of it is stored. A tool that is
    /// already running finishes.
    pub(super) fn stop(&self, chat_id: i64) -> Result<(), ErrorService> {
        let run = self.runs.lock().unwrap().get(&chat_id).cloned();
        match run {
            Some(run) => {
                run.stop();
                Ok(())
            }
            None => Err(ErrorService::new(StatusCode::CONFLICT, "the chat has no run to stop")),
        }
    }

    /// What the chat's turn is doing.
    pub(super) async fn state(&self, chat_id: i64) -> Result<TurnState, ErrorService> {
        let last_end = self.last_ends.lock().unwrap().get(&chat_id).cloned();
        let run = self.runs.lock().unwrap().get(&chat_id).cloned();
        if let Some(run) = run {
            let snapshot = run.snapshot();
            return Ok(TurnState {
                status: TurnStatus::Running,
                started_at: Some(snapshot.started_at),
                call_started_at: snapshot.call_started_at,
                eval_tokens: snapshot.eval_tokens,
                prompt_tokens: snapshot.prompt_tokens,
                step: snapshot.step,
                step_limit: snapshot.step_limit,
                running_tool: snapshot.running_tool,
                tool_started_at: snapshot.tool_started_at,
                pending: vec![],
                last_end,
            });
        }
        // Only for a chat with no run: while one goes on, a pending call is just between the model's
        // reply and its tool, and settling it as interrupted would be wrong
        let pending = self.tool_calls.can_use(chat_id).await?.tools;
        let waiting = Self::needs_permission(&pending);
        Ok(TurnState {
            status: if waiting { TurnStatus::WaitingForPermission } else { TurnStatus::Idle },
            started_at: None,
            call_started_at: None,
            eval_tokens: 0,
            prompt_tokens: None,
            step: 0,
            step_limit: None,
            running_tool: None,
            tool_started_at: None,
            pending: if waiting { pending } else { vec![] },
            last_end,
        })
    }

    /// Stores the prompt and answers it with a single model call, for a chat whose model has no tools
    /// (a messaging plugin's): nothing to loop over, nobody to stop it, no run to show.
    pub(super) async fn reply_once(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        think: Option<ThinkChoice>,
    ) -> Result<ChatOut, ErrorService> {
        self.store_user_message(chat_id, prompt, images, vec![]).await?;
        self.turn.make_room(chat_id, think.clone(), 0).await;
        let notices = self.notices.flush_job_notices(chat_id).await?;
        let messages = self.history.for_chat(chat_id).await?;
        let step = Step { chat_id, messages, think, notices, can_auto_continue: true, step_limit: None, run: RunTracker::new() };
        self.turn.step(step).await?.into_reply()
    }

    /// Runs a sub-agent's chat with `prompt` until it hands back an answer, stops, or runs out of model
    /// calls. Nothing here can ask the user anything: see `Policy::Subagent`.
    pub(super) async fn run_subagent(
        &self,
        chat_id: i64,
        prompt: String,
        think: Option<ThinkChoice>,
        auto_confirm: bool,
    ) -> Result<SubagentResult, ErrorService> {
        let slot = self.claim(chat_id)?;
        self.store_user_message(chat_id, prompt, vec![], vec![]).await?;
        let run = slot.run.clone();
        // A page that has the sub-agent's chat open follows it like any run
        self.events.publish(ServerEvent::RunStarted { chat_id, started_at: run.snapshot().started_at });
        let end = self.run(chat_id, think, Start::Subagent, Policy::Subagent, auto_confirm, None, run.clone()).await;
        drop(slot);
        self.finish(chat_id, &end, run.snapshot());
        match end {
            RunEnd::Subagent(result) => Ok(result),
            RunEnd::Failed(e) => Err(e),
            RunEnd::Stopped => Ok(SubagentResult::Incomplete("the sub-agent was stopped".to_string())),
            RunEnd::Answered | RunEnd::StepLimit | RunEnd::WaitingForPermission | RunEnd::NothingToDo => {
                Ok(SubagentResult::Incomplete("the sub-agent's run ended without a result".to_string()))
            }
        }
    }

    // ---- the run ----

    fn claim(&self, chat_id: i64) -> Result<RunSlot, ErrorService> {
        let mut runs = self.runs.lock().unwrap();
        if runs.contains_key(&chat_id) {
            return Err(ErrorService::new(StatusCode::CONFLICT, "the chat already has a run going on"));
        }
        let run = RunTracker::new();
        runs.insert(chat_id, run.clone());
        Ok(RunSlot { runs: self.runs.clone(), chat_id, run })
    }

    fn spawn(&self, slot: RunSlot, think: Option<ThinkChoice>, start: Start, policy: Policy) {
        let runner = self.clone();
        let chat_id = slot.chat_id;
        self.thinks.lock().unwrap().insert(chat_id, think.clone());
        tokio::spawn(async move {
            let run = slot.run.clone();
            // A wake-up announces itself once it knows there is a notice to answer
            if !matches!(start, Start::Wake) {
                runner.events.publish(ServerEvent::RunStarted { chat_id, started_at: run.snapshot().started_at });
            }
            let end = runner.run_user_run(chat_id, think, start, policy, run.clone()).await;
            // The chat is free before the event goes out: a page that asks for the state on `RunEnded`
            // must not find the run still there
            drop(slot);
            // A job that ended during the run's last model call found the chat busy and was not woken for:
            // look again now that the chat is free (a wake-up with nothing to report ends at once)
            let look_again = matches!(policy, Policy::Attended) && matches!(end, RunEnd::Answered | RunEnd::StepLimit);
            runner.finish(chat_id, &end, run.snapshot());
            if look_again {
                if let Err(e) = runner.wake(chat_id).await {
                    tracing::warn!(chat_id, "couldn't look for jobs finished during the run: {}", e.message.as_deref().unwrap_or("unknown error"));
                }
            }
        });
    }

    async fn run_user_run(&self, chat_id: i64, think: Option<ThinkChoice>, start: Start, policy: Policy, run: RunTracker) -> RunEnd {
        let user_id = match self.chat_store.chat(chat_id).await {
            Ok(chat) => chat.user_id,
            Err(e) => return RunEnd::Failed(e.into()),
        };
        let auto_confirm = self.settings_store.auto_confirm(user_id).await.unwrap_or(false);
        let max_steps = self.settings_store.max_turn_steps(user_id).await.unwrap_or(None);
        if matches!(policy, Policy::Attended) {
            run.set_step_limit(max_steps);
        }
        self.run(chat_id, think, start, policy, auto_confirm, max_steps, run).await
    }

    /// Records how a run ended and tells whoever is watching.
    fn finish(&self, chat_id: i64, end: &RunEnd, run: RunSnapshot) {
        let (reason, detail, status) = match end {
            RunEnd::Answered => (RunEndReason::Answered, None, None),
            RunEnd::StepLimit => (RunEndReason::StepLimit, None, None),
            RunEnd::Stopped => (RunEndReason::Stopped, None, None),
            RunEnd::WaitingForPermission => (RunEndReason::WaitingForPermission, None, None),
            RunEnd::Failed(e) => {
                let status = e.http_code.as_u16();
                let detail = e.message.clone().unwrap_or_else(|| "the run failed".to_string());
                tracing::warn!(chat_id, "run failed: {detail}");
                (RunEndReason::Failed, Some(detail), Some(status))
            }
            RunEnd::Subagent(SubagentResult::Answer(_)) => (RunEndReason::Answered, None, None),
            // It stopped without handing anything back (out of calls, never returned): shown as a failed run
            RunEnd::Subagent(SubagentResult::Incomplete(text)) => {
                let detail: String = text.chars().take(300).collect();
                (RunEndReason::Failed, Some(detail), None)
            }
            // Nothing happened, so there is nothing to report
            RunEnd::NothingToDo => return,
        };
        let ended = RunEnded { reason, detail: detail.clone(), ended_at: chrono::Utc::now(), started_at: run.started_at, eval_tokens: run.eval_tokens, status };
        self.last_ends.lock().unwrap().insert(chat_id, ended);
        self.events.publish(ServerEvent::RunEnded { chat_id, reason, detail, started_at: run.started_at, eval_tokens: run.eval_tokens, status });
    }

    /// The loop. A step is a model call and what comes of it; after a reply that asks for tools the
    /// tools run and the next step follows.
    #[allow(clippy::too_many_arguments)]
    async fn run(
        &self,
        chat_id: i64,
        think: Option<ThinkChoice>,
        start: Start,
        policy: Policy,
        auto_confirm: bool,
        max_steps: Option<u32>,
        run: RunTracker,
    ) -> RunEnd {
        match self.run_steps(chat_id, think, start, policy, auto_confirm, max_steps, run).await {
            Ok(end) => end,
            Err(e) => RunEnd::Failed(e),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_steps(
        &self,
        chat_id: i64,
        think: Option<ThinkChoice>,
        start: Start,
        policy: Policy,
        auto_confirm: bool,
        max_steps: Option<u32>,
        run: RunTracker,
    ) -> Result<RunEnd, ErrorService> {
        let mut first = true;
        let mut replace = None;
        let mut wake = false;
        match start {
            Start::Answer { decisions } => {
                // The answers come first: the calls they are about run before the model is asked anything
                match self.run_tools(chat_id, policy, auto_confirm, Some(&decisions), &run).await? {
                    ToolsOutcome::Stopped => return Ok(RunEnd::Stopped),
                    ToolsOutcome::NeedsPermission => return Ok(RunEnd::WaitingForPermission),
                    ToolsOutcome::Done { .. } => {}
                }
            }
            Start::Regenerate { replace: old } => replace = Some(old),
            Start::Wake => wake = true,
            Start::Prompt | Start::Subagent => {}
        }

        let mut steps = 0u32;
        let mut failed_return: Option<FailedReturn> = None;
        let mut reminded = false;
        loop {
            if run.is_stopped() {
                return Ok(RunEnd::Stopped);
            }

            self.turn.make_room(chat_id, think.clone(), 0).await;
            // A reply that replaces another is stored before the old one goes, and a notice flushed now would
            // land between them: it is left for the next step
            let notices = if replace.is_some() { vec![] } else { self.notices.flush_job_notices(chat_id).await? };
            if wake && first {
                if notices.is_empty() {
                    return Ok(RunEnd::NothingToDo);
                }
                self.events.publish(ServerEvent::RunStarted { chat_id, started_at: run.snapshot().started_at });
            }
            if !notices.is_empty() {
                self.events.publish(ServerEvent::MessagesChanged { chat_id });
            }
            let mut messages = self.history.for_chat(chat_id).await?;
            if replace.is_some() {
                // The reply being replaced is still stored; the model must not see it
                messages.pop();
            }

            let step_limit = match policy {
                Policy::Attended => max_steps.filter(|limit| steps + 1 >= *limit),
                Policy::Subagent => None,
            };
            run.set_step(steps + 1);
            let step = Step {
                chat_id,
                messages,
                think: think.clone(),
                notices,
                can_auto_continue: true,
                step_limit,
                run: run.clone(),
            };
            let out = match self.turn.step(step).await? {
                StepOut::Stopped => return Ok(RunEnd::Stopped),
                StepOut::Reply(out) => out,
            };
            steps += 1;
            first = false;
            if let Some(old) = replace.take() {
                self.chat_store.delete_message(chat_id, old).await?;
            }
            self.events.publish(ServerEvent::MessagesChanged { chat_id });

            if !out.can_use_tools {
                if matches!(policy, Policy::Subagent) {
                    // Stopping after a refused `llm.return_agent` isn't an answer: the last message is
                    // usually just "I will call it now". It gets one reminder, then what it wrote is
                    // handed over as it stands.
                    if let Some(failed) = &failed_return {
                        if reminded {
                            let text = prompts::subagent_never_returned(&failed.error, &failed.attempt, &out.content);
                            return Ok(RunEnd::Subagent(SubagentResult::Incomplete(text)));
                        }
                        reminded = true;
                        self.remind_to_return(chat_id, &failed.error).await?;
                        continue;
                    }
                    let result = match out.content.trim() {
                        "" => SubagentResult::Incomplete("the sub-agent stopped without writing an answer".to_string()),
                        answer => SubagentResult::Answer(answer.to_string()),
                    };
                    return Ok(RunEnd::Subagent(result));
                }
                return Ok(if step_limit.is_some() { RunEnd::StepLimit } else { RunEnd::Answered });
            }

            match self.run_tools(chat_id, policy, auto_confirm, None, &run).await? {
                ToolsOutcome::Stopped => return Ok(RunEnd::Stopped),
                ToolsOutcome::NeedsPermission => return Ok(RunEnd::WaitingForPermission),
                ToolsOutcome::Done { returned } => match returned {
                    Some(Returned::Answer(answer)) => return Ok(RunEnd::Subagent(SubagentResult::Answer(answer))),
                    Some(Returned::Refused(error)) => {
                        failed_return = Some(FailedReturn { error, attempt: out.content.clone() });
                    }
                    None => {}
                },
            }

            if matches!(policy, Policy::Subagent) && steps >= SUBAGENT_MAX_MODEL_CALLS {
                let text = prompts::subagent_out_of_calls(SUBAGENT_MAX_MODEL_CALLS as usize, &out.content);
                return Ok(RunEnd::Subagent(SubagentResult::Incomplete(text)));
            }
        }
    }

    // ---- tools ----

    /// Whether any of these calls is refused for want of a permission the user could give.
    fn needs_permission(pending: &[AgentToolCall]) -> bool {
        pending.iter().any(|call| matches!(call.permission, AgentToolPermission::Denied { escalation: Some(_), .. }))
    }

    async fn waiting_for_permission(&self, chat_id: i64) -> Result<bool, ErrorService> {
        Ok(Self::needs_permission(&self.tool_calls.can_use(chat_id).await?.tools))
    }

    /// Runs every tool call the model's last reply asked for, in order, recording each result. What a call
    /// that needs permission does depends on who is asking: with `decisions` (the user's answers) they are
    /// applied first; with auto-confirm everything the calls' escalations offer is granted up front (all
    /// of them, then the calls run — the reason `allow_scope` merges deltas instead of overwriting); otherwise
    /// an attended run stops here and a sub-agent's call is refused. A successful `llm.return_agent`
    /// ends the sub-agent's calls: whatever is queued behind it never executes.
    async fn run_tools(
        &self,
        chat_id: i64,
        policy: Policy,
        auto_confirm: bool,
        decisions: Option<&[Decision]>,
        run: &RunTracker,
    ) -> Result<ToolsOutcome, ErrorService> {
        let pending = self.tool_calls.pending_tool_calls(chat_id).await?;
        let mut views = Vec::with_capacity(pending.len());
        for call in pending {
            views.push(self.tool_calls.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?);
        }

        // The call the run has to wait at, when an attended run meets one that needs the user's permission
        let mut stop_before: Option<usize> = None;
        // One-time scopes, by the position of the call they are for
        let mut one_time: HashMap<usize, Value> = HashMap::new();
        match decisions {
            Some(decisions) => {
                for decision in decisions {
                    let Some(AgentToolPermission::Denied { escalation: Some(grant), .. }) =
                        views.get(decision.index).map(|view| &view.permission)
                    else {
                        continue;
                    };
                    match decision.allowance {
                        Allowance::Permanent => {
                            self.tool_calls.allow_scope(chat_id, views[decision.index].name.clone(), grant.scope.clone()).await?;
                        }
                        Allowance::OnlyNow => {
                            one_time.insert(decision.index, grant.scope.clone());
                        }
                        Allowance::Deny => {}
                    }
                }
            }
            None if Self::needs_permission(&views) => {
                if auto_confirm {
                    for view in &views {
                        if let AgentToolPermission::Denied { escalation: Some(grant), .. } = &view.permission {
                            self.tool_calls.allow_scope(chat_id, view.name.clone(), grant.scope.clone()).await?;
                        }
                    }
                } else if matches!(policy, Policy::Attended) {
                    // The calls before the first one that needs permission run now, in the order they were asked,
                    // and the run waits there: pausing before them would leave them pending, and a read of the
                    // chat would take them for calls a crash had cut short
                    stop_before = views.iter().position(|view| matches!(view.permission, AgentToolPermission::Denied { escalation: Some(_), .. }));
                }
            }
            None => {}
        }

        let unattended = matches!(policy, Policy::Subagent);
        let mut returned = None;
        for index in 0..views.len() {
            if run.is_stopped() {
                return Ok(ToolsOutcome::Stopped);
            }
            if stop_before == Some(index) {
                return Ok(ToolsOutcome::NeedsPermission);
            }
            self.events.publish(ServerEvent::ToolStarted { chat_id, tool_name: views[index].name.clone() });
            run.tool_started(&views[index].name);
            let result = self.tool_calls.run_next_tool(chat_id, one_time.remove(&index), unattended).await;
            run.tool_finished();
            let out = result?;
            self.events.publish(ServerEvent::MessagesChanged { chat_id });

            if out.tool_name == ReturnAgentTool::NAME {
                if out.success {
                    let answer = match out.content {
                        Value::String(text) => text,
                        other => other.to_string(),
                    };
                    return Ok(ToolsOutcome::Done { returned: Some(Returned::Answer(answer)) });
                }
                returned = Some(Returned::Refused(out.err.unwrap_or_else(|| "the call was refused".to_string())));
            }
            if out.tools.is_empty() {
                break;
            }
        }
        Ok(ToolsOutcome::Done { returned })
    }

    // ---- messages the runner writes ----

    async fn store_user_message(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        file_ids: Vec<i64>,
    ) -> Result<i64, ErrorService> {
        let stored = self
            .chat_store
            .new_message(NewMessage {
                chat_id,
                role: "user".to_string(),
                content: prompt,
                tool_name: None,
                thinking: None,
                thought_duration_ms: None,
                tool_success: None,
                tool_denied: false,
                tool_calls: vec![],
                images,
                file_ids,
                prompt_tokens: None,
                eval_tokens: None,
                timings: MessageTimings::default(),
            })
            .await?;
        self.events.publish(ServerEvent::MessagesChanged { chat_id });
        Ok(stored.id)
    }

    /// Tells the sub-agent its `llm.return_agent` call was refused and it isn't done. Stored as a
    /// `notice` (sent to the model as a user turn, shown as a muted marker), the same way the
    /// continuation prompt after a cut-off thought is.
    async fn remind_to_return(&self, chat_id: i64, error: &str) -> Result<(), ErrorService> {
        self.chat_store
            .new_message(NewMessage {
                chat_id,
                role: "notice".to_string(),
                content: prompts::return_reminder(error),
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facade::agent::AgentScopeGrant;
    use serde_json::json;

    fn call(permission: AgentToolPermission) -> AgentToolCall {
        AgentToolCall { permission, name: "storage.write_file".to_string(), arguments: json!({}) }
    }

    fn denied(escalation: bool) -> AgentToolPermission {
        AgentToolPermission::Denied {
            reason: "not allowed".to_string(),
            escalation: escalation.then(|| AgentScopeGrant { scope: json!({}), ui_message: "Allow?".to_string() }),
        }
    }

    #[test]
    fn only_a_refusal_the_user_could_lift_makes_a_run_wait() {
        assert!(!TurnRunner::needs_permission(&[]));
        assert!(!TurnRunner::needs_permission(&[call(AgentToolPermission::Allowed)]));
        // A hard refusal is recorded for the model, nobody is asked
        assert!(!TurnRunner::needs_permission(&[call(denied(false))]));
        assert!(TurnRunner::needs_permission(&[call(AgentToolPermission::Allowed), call(denied(true))]));
    }
}
