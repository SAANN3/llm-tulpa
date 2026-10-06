//! The model's tool calls for a chat: which are still waiting to run, whether each is permitted
//! (the grants stored for the chat), running the next one and recording its result, and which
//! chats have a call executing right now.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use serde_json::Value;

use super::{AgentScopeGrant, AgentToolCall, AgentToolPermission, CanUseTool, UseToolOut};
use super::model_call::ModelCall;
use super::prompts::{self, INTERRUPTED_TOOL_MESSAGE};
use crate::services::chat_store::{ChatStore, MessageTimings, NewMessage, ToolCallOut};
use crate::services::error::ErrorService;
use crate::services::permission_store::{PermissionStore, PermissionStoreErrors};
use crate::services::tools::ToolService;
use crate::tools::base::{ResolvedScope, Tool, ToolContext, ToolPermission};
use crate::tools::subagent;

/// The most of the context window one tool result may take, as a fraction of it in tokens. A result past this
/// is stored cut (head kept, with a line saying so): a message that is by itself bigger than the window can't
/// be folded away, and leaves the chat unable to send anything. The tools' own limits (a file read stops at
/// 40,000 characters) sit below this on a normal window, so this is the backstop for the ones without one.
const RESULT_WINDOW_FRACTION: f64 = 0.25;
/// Characters a token is counted as when turning that into a size: low on purpose, see `Turn`'s estimate.
const RESULT_CHARS_PER_TOKEN: f64 = 3.0;
/// A window so small that the fraction would cut results to nothing still lets this much through.
const MIN_RESULT_CAP_CHARS: usize = 4_000;

/// How many of a chat's newest messages `pending_tool_calls` looks through: it walks back over the
/// tool results of the last reply, a handful at most.
const PENDING_TOOL_CALLS_LOOKBACK: u64 = 500;

/// Marks a chat as having a tool call executing, for as long as it lives. Dropping it — on
/// completion, on an error, or because the request was cancelled (a client that disconnects drops
/// the handler's future) — clears the mark. Without it a chat with an *allowed* call in flight is
/// indistinguishable from one whose call was cut short by a restart: both look like "allowed and
/// still unresolved". The same guard marks a sub-agent's chat for its whole run
/// (`live_subagents`), for the same reason.
pub(super) struct RunningToolGuard {
    running: Arc<Mutex<HashSet<i64>>>,
    chat_id: i64,
}

impl Drop for RunningToolGuard {
    fn drop(&mut self) {
        self.running.lock().unwrap().remove(&self.chat_id);
    }
}

#[derive(Clone)]
pub(super) struct ToolCalls {
    chat_store: Arc<ChatStore>,
    tools: Arc<ToolService>,
    /// Per-chat tool-permission grants — what scope each tool has already been given
    /// within a given chat, if any. Consulted by `to_agent_tool_call`/`run_next_tool` to
    /// decide whether a call is `Allowed` outright or needs the caller to confirm.
    permission_store: Arc<PermissionStore>,
    /// Template `run_next_tool` calls `copy_with_chat_id` on to get the real, per-call
    /// context — its own `chat_id` is unused/meaningless (never itself handed to a
    /// tool). See `ToolContext`'s own doc comment for why it isn't `AppState`.
    tool_context: ToolContext,
    /// Which context window a chat runs under, for the cap on a result.
    model: ModelCall,
    /// Chats with a tool call executing right now. See `RunningToolGuard`.
    running: Arc<Mutex<HashSet<i64>>>,
    /// Sub-agent chats whose run is going on right now (from being started until it ends or is
    /// killed). Their tool calls are the backend's to run, so `is_running` treats them as busy —
    /// without it, opening one between two of its calls would look like a call cut short by a restart.
    live_subagents: Arc<Mutex<HashSet<i64>>>,
}

impl ToolCalls {
    pub(super) fn new(
        chat_store: Arc<ChatStore>,
        tools: Arc<ToolService>,
        permission_store: Arc<PermissionStore>,
        tool_context: ToolContext,
        model: ModelCall,
    ) -> Self {
        Self {
            chat_store,
            tools,
            permission_store,
            tool_context,
            model,
            running: Arc::new(Mutex::new(HashSet::new())),
            live_subagents: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Claims the chat's single tool-execution slot. Two calls running at once for one chat (a
    /// second tab opened, or a reload, while a long command is still going) would run the same
    /// pending call twice.
    fn mark_running(&self, chat_id: i64) -> Result<RunningToolGuard, ErrorService> {
        if !self.running.lock().unwrap().insert(chat_id) {
            return Err(ErrorService::new(StatusCode::CONFLICT, "a tool call is already running for this chat"));
        }
        Ok(RunningToolGuard { running: self.running.clone(), chat_id })
    }

    pub(super) fn is_running(&self, chat_id: i64) -> bool {
        self.running.lock().unwrap().contains(&chat_id) || self.live_subagents.lock().unwrap().contains(&chat_id)
    }

    /// The most characters of one tool result that are stored for a chat on a window of `context` tokens.
    fn result_cap_chars(context: u64) -> usize {
        ((context as f64 * RESULT_WINDOW_FRACTION * RESULT_CHARS_PER_TOKEN) as usize).max(MIN_RESULT_CAP_CHARS)
    }

    /// What is stored for a tool's result: its JSON as it is, or, past `cap_chars`, the start of it as a JSON
    /// string with the line `prompts::tool_result_cut` added. The caller still gets the whole result.
    fn capped(content: &Value, cap_chars: usize) -> String {
        let text = content.to_string();
        let total = text.chars().count();
        if total <= cap_chars {
            return text;
        }
        let head: String = text.chars().take(cap_chars).collect();
        Value::String(format!("{head}\n{}", prompts::tool_result_cut(cap_chars, total))).to_string()
    }

    /// Gives a sub-agent's chat the grants its parent chat has.
    pub(super) async fn copy_grants(&self, parent_chat_id: i64, sub_chat_id: i64) -> Result<(), ErrorService> {
        Ok(self.permission_store.copy_grants(parent_chat_id, sub_chat_id).await?)
    }

    /// Marks a sub-agent's chat as live until the returned guard is dropped — the same guard type
    /// the tool-execution mark uses, over the set `is_running` also consults.
    pub(super) fn mark_live(&self, chat_id: i64) -> RunningToolGuard {
        self.live_subagents.lock().unwrap().insert(chat_id);
        RunningToolGuard { running: self.live_subagents.clone(), chat_id }
    }

    /// Records the tool calls that were cut short as interrupted, instead of leaving them to be
    /// run again. A turn starting on the chat is where this is done: a call the chat's grants already
    /// allow is executed the moment the model asks for it, never left waiting for a person — so
    /// one that is *still* unresolved while nothing is executing was interrupted (the backend
    /// restarted, or the client went away mid-run), and re-running it could repeat something that
    /// already happened, or block again on a command that never exits. A call waiting for the
    /// user's confirmation is different and stays as it is; so does everything queued after it,
    /// since results are recorded in the order the model asked.
    pub(super) async fn settle_interrupted(&self, chat_id: i64) -> Result<(), ErrorService> {
        if self.is_running(chat_id) {
            return Ok(());
        }

        for call in self.pending_tool_calls(chat_id).await? {
            let name = call.tool_name.clone();
            let view = self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?;
            if !matches!(view.permission, AgentToolPermission::Allowed) {
                break;
            }

            self.chat_store
                .new_message(NewMessage {
                    chat_id,
                    role: "tool".to_string(),
                    content: Value::String(INTERRUPTED_TOOL_MESSAGE.to_string()).to_string(),
                    tool_name: Some(name),
                    thinking: None,
                    thought_duration_ms: None,
                    tool_success: Some(false),
                    tool_denied: false,
                    tool_calls: vec![],
                    images: vec![],
                    file_ids: vec![],
                    prompt_tokens: None,
                    eval_tokens: None,
                    timings: MessageTimings::default(),
                })
                .await?;
        }
        Ok(())
    }

    /// The tool calls the model has asked for that haven't been run yet, without
    /// actually running them — lets a caller check each one's `permission` (and warn
    /// about a `Denied` one) before committing to `use_tool`.
    ///
    /// Only reads: calls that were cut short are recorded as interrupted when a turn starts (see
    /// `settle_interrupted`). While a call is executing, nothing is reported as pending — whoever is
    /// running it owns it.
    pub(super) async fn can_use(&self, chat_id: i64) -> Result<CanUseTool, ErrorService> {
        if self.is_running(chat_id) {
            return Ok(CanUseTool { can_use: false, tools: vec![] });
        }

        let pending = self.pending_tool_calls(chat_id).await?;

        let mut tools = Vec::with_capacity(pending.len());
        for call in pending {
            tools.push(self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?);
        }

        Ok(CanUseTool {
            can_use: !tools.is_empty(),
            tools,
        })
    }

    /// `use_tool`'s body. `unattended` is for a chat nobody is watching (a sub-agent's): a denied
    /// call is reported to the model as final for this run, instead of promising it "they'll be
    /// asked to approve it then" — there is no one to ask.
    pub(super) async fn run_next_tool(&self, chat_id: i64, scope: Option<Value>, unattended: bool) -> Result<UseToolOut, ErrorService> {
        let _running = self.mark_running(chat_id)?;
        let chat = self.chat_store.chat(chat_id).await?;
        let is_subagent = chat.parent_chat_id.is_some();
        let mut pending = self.pending_tool_calls(chat_id).await?.into_iter();
        let next = pending
            .next()
            .ok_or_else(|| ErrorService::new(StatusCode::BAD_REQUEST, "no pending tool call to run"))?;

        let had_scope;
        let effective_scope = match scope {
            Some(scope) => {
                had_scope = true;
                match self.tools.get_tool(&next.tool_name).await {
                    Some(tool) => resolved_scope_from_json(tool.as_ref(), scope),
                    None => ResolvedScope::default(),
                }
            }
            None => {
                let stored = self.stored_scope(chat_id, &next.tool_name).await?;
                had_scope = stored.own.is_some() || !stored.shared.is_empty();
                stored
            }
        };

        let result_cap = Self::result_cap_chars(self.model.context_of(&chat).await?);

        let permission = self
            .tool_permission(&next.tool_name, next.arguments.clone(), effective_scope, is_subagent)
            .await;

        let (success, denied, err, content) = match permission {
            AgentToolPermission::Allowed => {
                let ctx = self.tool_context.copy_with_chat_id(chat_id, chat.user_id, chat.provider, chat.model);
                match self.tools.call_tool(&next.tool_name, next.arguments, &ctx).await {
                    Ok(value) => (true, false, None, value),
                    Err(e) => {
                        let message = e.to_string();
                        (false, false, Some(message.clone()), Value::String(message))
                    }
                }
            }
            AgentToolPermission::Denied { reason, escalation } => {
                let message = prompts::tool_call_refused(unattended, had_scope, escalation.is_some(), &next.tool_name, &reason);
                (false, true, Some(message.clone()), Value::String(message))
            }
        };

        let stored = self
            .chat_store
            .new_message(NewMessage {
                chat_id,
                role: "tool".to_string(),
                content: Self::capped(&content, result_cap),
                tool_name: Some(next.tool_name.clone()),
                thinking: None,
                thought_duration_ms: None,
                tool_success: Some(success),
                tool_denied: denied,
                tool_calls: vec![],
                images: vec![],
                file_ids: vec![],
                prompt_tokens: None,
                eval_tokens: None,
                timings: MessageTimings::default(),
            })
            .await?;

        let mut tools = Vec::with_capacity(pending.len());
        for call in pending {
            tools.push(self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?);
        }

        Ok(UseToolOut {
            id: stored.id,
            success,
            denied,
            tool_name: next.tool_name,
            err,
            content,
            created_at: stored.created_at,
            tools,
        })
    }

    /// Persists a scope grant for a tool within a chat, so future calls to that tool (or
    /// any other tool sharing one of its buckets — see `Tool::shared_buckets`) can be
    /// `Allowed` without asking again. `scope` is the envelope `resolved_scope_to_json`
    /// produced when this grant was first offered, echoed back verbatim by the
    /// frontend; `resolved_scope_from_json` reads it back into its own/shared-bucket
    /// deltas — each one is just the single new fact that call needed (see
    /// `storage::check_scope`), not a snapshot of everything already granted. Each delta
    /// is appended to that row's *current* value, read fresh right here rather than
    /// trusted from whatever the caller last saw: two denied calls from the same reply
    /// needing the same bucket have their escalations computed from the same
    /// pre-approval state, so if this just overwrote with the caller's delta, approving
    /// the second would erase the first's grant. Reading fresh at the moment each one is
    /// actually persisted is what makes approving both, in sequence, correct.
    pub(super) async fn allow_scope(&self, chat_id: i64, tool_name: String, scope: Value) -> Result<(), ErrorService> {
        let Some(tool) = self.tools.get_tool(&tool_name).await else {
            return Err(ErrorService::new(
                StatusCode::BAD_REQUEST,
                format!("no tool named '{tool_name}'"),
            ));
        };

        let delta = resolved_scope_from_json(tool.as_ref(), scope);

        if let Some(own_delta) = delta.own {
            let existing = self.get_scope_or_none(chat_id, &tool_name).await?;
            self.permission_store.update_scope(chat_id, &tool_name, merge_scope_delta(existing, own_delta)).await?;
        }
        for (bucket, shared_delta) in delta.shared {
            let existing = self.get_scope_or_none(chat_id, bucket.db_key()).await?;
            self.permission_store
                .update_scope(chat_id, bucket.db_key(), merge_scope_delta(existing, shared_delta))
                .await?;
        }

        Ok(())
    }

    /// A tool's actual scope for one call — its own bucket (if it has one) plus every
    /// shared bucket it declares, each fetched and kept separate rather than flattened
    /// into one object. Flattening would collide: every storage bucket stores its grant
    /// under the same JSON key (`SharedBucket::json_key`), so a tool declaring both
    /// `StorageRead` and `StorageWrite` would have one silently overwrite the other if
    /// they were merged into a single map instead of kept apart by bucket.
    async fn stored_scope(&self, chat_id: i64, tool_name: &str) -> Result<ResolvedScope, ErrorService> {
        let Some(tool) = self.tools.get_tool(tool_name).await else {
            return Ok(ResolvedScope::default());
        };

        let own = if tool.uses_own_bucket() {
            self.get_scope_or_none(chat_id, tool_name).await?
        } else {
            None
        };

        let mut shared = HashMap::new();
        for &bucket in tool.shared_buckets() {
            if let Some(value) = self.get_scope_or_none(chat_id, bucket.db_key()).await? {
                shared.insert(bucket, value);
            }
        }

        Ok(ResolvedScope { own, shared })
    }

    /// `PermissionStore::get_scope`, with "nothing granted yet" collapsed to `None`
    /// rather than an error — that's the normal/expected case for most tool calls, not
    /// a failure.
    async fn get_scope_or_none(&self, chat_id: i64, key: &str) -> Result<Option<Value>, ErrorService> {
        match self.permission_store.get_scope(chat_id, key).await {
            Ok(scope) => Ok(Some(scope)),
            Err(PermissionStoreErrors::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Resolves a tool call's permission into the facade-facing shape: an unknown tool
    /// name and a `ToolSerializationError` (couldn't even read `data`) both collapse
    /// into `Denied` with no escalation — from a caller's perspective both just mean
    /// "this can't run as given," the distinction between them only matters to
    /// whoever's implementing the tool. A `Denied` from the tool itself always carries
    /// its `reason` through, whether or not it came with an `escalation` — a hard
    /// refusal still needs to reach the UI and the model, not just silently vanish.
    async fn tool_permission(
        &self,
        tool_name: &str,
        data: Value,
        scope: ResolvedScope,
        is_subagent: bool,
    ) -> AgentToolPermission {
        if !subagent::available_to(tool_name, is_subagent) {
            return AgentToolPermission::Denied {
                reason: format!("'{tool_name}' isn't available in this chat"),
                escalation: None,
            };
        }

        let Some(tool) = self.tools.get_tool(tool_name).await else {
            return AgentToolPermission::Denied {
                reason: format!("no tool named '{tool_name}'"),
                escalation: None,
            };
        };

        match tool.is_dangerous(data, scope) {
            Ok(ToolPermission::Allowed) => AgentToolPermission::Allowed,
            Ok(ToolPermission::Denied { reason, escalation }) => AgentToolPermission::Denied {
                reason,
                escalation: escalation.map(|grant| AgentScopeGrant {
                    scope: resolved_scope_to_json(grant.scope),
                    ui_message: grant.ui_message,
                }),
            },
            Err(e) => AgentToolPermission::Denied {
                reason: format!("couldn't validate call arguments: {e}"),
                escalation: None,
            },
        }
    }

    /// Finds tool calls the model has requested but that don't have a result message
    /// yet, by walking the chat's messages newest-first: skip past `tool` rows (already
    /// resolved), and whatever comes right after them decides the answer. If that's an
    /// `assistant` message with `tool_calls`, the ones beyond however many `tool` rows
    /// we just skipped are still pending (`tool_calls` is id-ordered, i.e. the order the
    /// model requested them in). Anything else — a plain assistant reply, a `user`
    /// message, or an empty chat — means nothing is pending; in particular a fresh
    /// `user` message always wins even if older unresolved tool calls sit further back,
    /// since the user talking again supersedes them.
    pub(super) async fn pending_tool_calls(&self, chat_id: i64) -> Result<Vec<ToolCallOut>, ErrorService> {
        let (messages, _) = self.chat_store.messages(chat_id, PENDING_TOOL_CALLS_LOOKBACK, 0).await?;

        let resolved = messages.iter().take_while(|message| message.role == "tool").count();

        let Some(candidate) = messages.get(resolved) else {
            return Ok(vec![]);
        };

        if candidate.role != "assistant" {
            return Ok(vec![]);
        }

        Ok(candidate.tool_calls[resolved.min(candidate.tool_calls.len())..].to_vec())
    }

    /// Builds the facade-facing view of a tool call the model has requested, including
    /// whether it's actually permitted right now — always checked against whatever
    /// scope is already stored for this chat/tool (never a one-time override; only
    /// `use_tool` accepts one of those), since this is a preview, not a commitment to
    /// run anything.
    pub(super) async fn to_agent_tool_call(
        &self,
        chat_id: i64,
        name: String,
        arguments: Value,
    ) -> Result<AgentToolCall, ErrorService> {
        let scope = self.stored_scope(chat_id, &name).await?;
        let is_subagent = self.chat_store.chat(chat_id).await?.parent_chat_id.is_some();
        let permission = self.tool_permission(&name, arguments.clone(), scope, is_subagent).await;

        Ok(AgentToolCall {
            permission,
            name,
            arguments,
        })
    }
}

/// Serializes a `ResolvedScope` into the flat JSON value that crosses the HTTP boundary
/// as `AgentScopeGrant.scope` — opaque to the frontend (`unknown` on its side), which
/// only ever echoes it back verbatim via `allow_scope` or a `use_tool` one-time
/// override. Shared buckets are keyed by `SharedBucket::db_key()` — the same string
/// `PermissionStore` rows already use — so `resolved_scope_from_json` (below) can read
/// them back into the right bucket unambiguously, rather than guessing a flattened
/// object apart by a shared JSON key the way the old `split_scope` had to.
fn resolved_scope_to_json(scope: ResolvedScope) -> Value {
    let shared: serde_json::Map<String, Value> =
        scope.shared.into_iter().map(|(bucket, value)| (bucket.db_key().to_string(), value)).collect();

    serde_json::json!({ "own": scope.own, "shared": shared })
}

/// The inverse of `resolved_scope_to_json`. `tool` decides which shared buckets are even
/// meaningful for it; a bucket key present in `json` that `tool` doesn't declare (e.g.
/// stale data from before a tool's declared buckets changed) is just dropped rather than
/// erroring — there's nothing sensible to do with it, and dropping it is no worse than
/// the grant never having existed.
fn resolved_scope_from_json(tool: &dyn Tool, json: Value) -> ResolvedScope {
    let own = json.get("own").filter(|v| !v.is_null()).cloned();

    let mut shared = HashMap::new();
    if let Some(shared_obj) = json.get("shared").and_then(|s| s.as_object()) {
        for &bucket in tool.shared_buckets() {
            if let Some(value) = shared_obj.get(bucket.db_key()) {
                shared.insert(bucket, value.clone());
            }
        }
    }

    ResolvedScope { own, shared }
}

/// Appends `delta`'s facts into `existing`, one level deep: for a top-level key that's
/// an object on both sides (e.g. `"folders"`), the two objects' own keys are unioned —
/// two different approved folders both end up in the same map, rather than the second
/// replacing the first. Anything else in `delta` just sets that key outright. Every
/// bucket's stored shape today is exactly one level deep (`{"folders": {...}}`,
/// `{"hosts": {...}}`), so one level of recursion covers everything currently in play.
fn merge_scope_delta(existing: Option<Value>, delta: Value) -> Value {
    let mut base = existing.and_then(|v| v.as_object().cloned()).unwrap_or_default();
    let Some(delta_obj) = delta.as_object() else {
        return delta;
    };

    for (key, delta_value) in delta_obj {
        match (base.get(key).and_then(|v| v.as_object()), delta_value.as_object()) {
            (Some(existing_inner), Some(delta_inner)) => {
                let mut merged_inner = existing_inner.clone();
                merged_inner.extend(delta_inner.clone());
                base.insert(key.clone(), Value::Object(merged_inner));
            }
            _ => {
                base.insert(key.clone(), delta_value.clone());
            }
        }
    }

    Value::Object(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_cap_follows_the_window_with_a_floor() {
        assert_eq!(ToolCalls::result_cap_chars(100_000), 75_000);
        assert_eq!(ToolCalls::result_cap_chars(16_384), 12_288);
        assert_eq!(ToolCalls::result_cap_chars(2_000), MIN_RESULT_CAP_CHARS);
    }

    #[test]
    fn a_result_within_the_cap_is_stored_as_it_is_and_a_larger_one_is_cut_with_a_note() {
        let small = json!({"content": "short"});
        assert_eq!(ToolCalls::capped(&small, 1_000), small.to_string());

        let big = json!({"content": "é".repeat(5_000)});
        let stored = ToolCalls::capped(&big, 1_000);
        let Value::String(text) = serde_json::from_str::<Value>(&stored).unwrap() else { panic!("a cut result is a JSON string") };
        assert!(text.starts_with("{\"content\":\"éé"));
        assert!(text.contains("1000 of"));
        assert!(text.chars().count() < 1_400);
    }
}
