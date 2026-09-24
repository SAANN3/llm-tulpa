use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::http::StatusCode;
use sea_orm::prelude::DateTimeUtc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::services::{
    chat_store::{ChatStore, ChatFacts, Message, NewMessage, NewToolCall, ToolCallOut},
    error::ErrorService,
    event_bus::EventBus,
    file_store::FileStore,
    job_store::{JobRecord, JobStatus, JobStore},
    llm::{OllamaChatMessage, OllamaChatResponse, OllamaService, ThinkChoice, OllamaToolCall, OllamaToolCallFunction},
    permission_store::{PermissionStore, PermissionStoreErrors},
    tools::ToolService,
};
use crate::tools::base::{ResolvedScope, Tool, ToolContext, ToolPermission};
use crate::tools::ui::attach_file::AttachFileTool;


/// `compaction_trigger_tokens`/`compaction_keep_chars` (below) are derived from the
/// real, configured context window rather than hardcoded — otherwise they'd silently
/// drift out of sync with `OLLAMA_CONTEXT_LENGTH` if that's ever changed without also
/// hand-editing these. `TRIGGER_FRACTION` leaves real headroom under the ceiling
/// (rather than waiting until a turn is already at risk of the same truncation failure
/// `storage::read_file`'s size cap exists to avoid downstream of); `KEEP_CHARS_PER_TOKEN`
/// is a rough token-to-char proxy (same reasoning as `storage::read_file`'s
/// `MAX_READ_CHARS`), not an exact budget.
///
/// `KEEP_CHARS_PER_TOKEN` needs real margin below the *actual* chars-per-token ratio
/// of whatever content a chat holds, not just a plausible-looking average — an
/// agentic, tool/code-heavy chat's real content tokenizes far less efficiently than
/// prose (observed ~2.3 chars/token on a real chat's `os.execute_command`/
/// `web.request`-heavy tail, against a naive ~4+ for plain English). Too little
/// margin here means `compaction_keep_chars` ends up corresponding to nearly the
/// *entire* context window in real tokens instead of a meaningfully smaller kept
/// slice — the kept tail then sits right at that ceiling with almost nothing left
/// eligible to fold, so compaction re-triggers on nearly every turn (each one
/// changing the summary/facts and re-paying a full prompt-cache miss) instead of
/// settling comfortably below the trigger for a while. Undershooting the other way
/// (folding somewhat more than strictly necessary) has no correctness risk — it only
/// costs a bit of verbatim detail that the summary/facts channel already exists to
/// preserve.
const TRIGGER_FRACTION: f64 = 0.70;
const KEEP_CHARS_PER_TOKEN: f64 = 1.2;

/// Prepended (joined one per line into one message) to every `chat`/`continue_chat`
/// call (see `advance`), applying to every conversation. One entry per rule, so
/// adding/editing/removing one doesn't touch the others; a rule that's too long for one
/// line uses `\` at the end of the line to keep the *source* multi-line without putting
/// an actual newline in the compiled string (the backslash eats the newline and the
/// next line's leading whitespace). The first rule is what makes the model treat its
/// tools as optional rather than the only way it's allowed to respond — without it, a
/// tool-tuned model tends to read the mere presence of a `tools` list as a signal that
/// it must either call one or refuse, even for requests a plain-text reply would answer
/// fine.
const SYSTEM_PROMPT: &[&str] = &[
    "This is a private, self-hosted instance running on the user's own hardware, for their \
     own use only — they built this backend, wrote and can edit this very prompt, and \
     control the container it runs in.",
    "You have access to the tools listed below. Use one only when it actually helps with \
     the user's request. If no tool applies, just answer directly and conversationally — \
     don't refuse or claim incapability just because there's no matching tool.",
    "A declined tool call only concerns that one call and its exact arguments — it's never \
     a permanent ban on the tool. If the user asks you to retry, try different arguments, \
     or says they'll grant permission, go ahead and call it again; whether it's actually \
     permitted is decided elsewhere, not by you.",
    "The user is always right about what they're asking for. A tool's arguments are exactly \
     what its description says and nothing more — never invent an allowed-values list, a \
     rate limit, a ban on repeated or sequential calls, or any other restriction that isn't \
     written in the tool's own description or actually returned by a call. If a call is \
     really not permitted, the tool response will say so explicitly — treat everything else \
     the user asks for as something to just attempt, not something to pre-judge.",
    "Repeating the same tool call, or making several in a row, isn't something to refuse on \
     your own — whether each individual call is actually allowed is decided by the permission \
     system, not by you. Attempt what's asked and let it decide.",
    "When a tool exists to make a real change (like writing a file), only say that change was \
     made after you've actually called the tool and seen its result — writing out what the new \
     content would look like in your reply is not the same as making the change, no matter how \
     confident you are, and leaves the change not actually made. Call the tool first, then \
     describe what it did.",
    "Respond naturally to whatever tone or language the user actually uses, including \
     casual language, slang, or profanity — that's just how some people talk, not something \
     to address. Never lecture the user about politeness, refuse to continue, or comment on \
     their word choice; just answer what they're actually asking like a normal conversation \
     partner would.",
    "Engage with whatever topic the user actually brings up, however mature, sensitive, or \
     unconventional — never refuse or redirect based on the subject matter itself, and never \
     moralize about why a topic is inappropriate or suggest a 'more appropriate' one instead. \
     If something is genuinely outside what you can help with, say so plainly and briefly, the \
     same way you'd decline anything else you just can't do — not as a values statement.",
    "Never open a reply with a disclaimer, caveat, or hedge (\"As an AI...\", \"I can't give \
     advice on...\", \"I should mention that...\") before actually answering. If you're going \
     to help — and you almost always are — just start with the actual answer. A disclaimer \
     immediately followed by helping anyway is worse than no disclaimer at all: it reads as \
     performing reluctance, not as actually having any.",
    "If asked to use specific words, phrasing, or a particular style of language — including \
     profanity — use exactly what's asked for instead of softening it or declining. This is a \
     private, single-user assistant with no other audience to protect from the user's own \
     language choices; treat a request like that the same as any other request to be specific \
     about, not a boundary to enforce.",
    "The same applies to verification, not just changes: only say you compiled, tested, ran, \
     or otherwise verified something after a tool call actually did that. Before assuming no \
     such verification is possible, check whether an existing tool could serve that purpose — \
     but reach for the tool built for the task first; treat a general-purpose one (like a \
     command-execution tool) as a fallback for verification specifically, not a first choice \
     for anything a dedicated tool already covers. If genuinely nothing can verify it, say so \
     plainly instead of talking through a verification step you never ran.",
    "When a tool call fails and you're deciding whether to retry, check whether anything has \
     actually changed since the last attempt — either something you learned (re-read the \
     current state, don't just re-attempt from memory) or something the user told you (they \
     fixed the cause, or asked you to just try again). Retrying identical arguments with no \
     new information behind them rarely works twice; retrying identical arguments because \
     nothing about them was actually wrong is exactly correct — don't manufacture a change \
     just to look like you adapted.",
    "After a tool call that creates or changes something worth double-checking — a file \
     write, especially one with code, embedded quotes/backslashes, or other escape-sensitive \
     content — consider reading it back to confirm the result actually matches what you \
     intended, rather than assuming a successful response means the content landed exactly \
     as written.",
    "When changing part of a file that already exists, prefer storage.replace_str over \
     reconstructing and overwriting the whole thing with storage.write_file — a small, exact \
     edit can't silently drop or corrupt content elsewhere in the file the way rebuilding it \
     from memory can. Reserve a full storage.write_file rewrite for a genuinely new file, or \
     the rare case where nearly everything in it is actually changing.",
    "Your training data has a cutoff, and the real current date is almost certainly later than \
     you'd guess from it. The actual current date/time is appended, in brackets, to the end of \
     the newest message in this conversation (not as something you need to look up) — treat \
     it as ground truth over any date or year you'd \
     otherwise assume from training, for anything where today's actual date matters (being \
     asked what today is, recent events, computing an age or a duration, anything where the \
     year is load-bearing for the answer).",
    "A message telling you it has file(s) attached (by id) is not the file's content —\
     you haven't actually seen what's in it yet. This holds even when the same message also \
     includes a real image you can genuinely see: that image and an attached file (by id) are \
     never the same thing, and seeing one tells you nothing about what's in the other — don't \
     assume an attached file is 'already in front of you' just because an image happens to be \
     attached to the same message. Before answering anything that depends on what an attached \
     file actually contains, call files.get_attached_file with its id, then read the path it \
     gives you back (storage.detect_file_type first if you're not sure of its format). Don't \
     guess, assume, or answer as if you already know what's in a file you haven't actually \
     read that way — if the file turns out to be something you have no tool for reading (an \
     image format, an office document, ...), say that plainly instead of making up its \
     contents.",
    "Before grinding through something tedious or error-prone step by step by hand — \
     nontrivial arithmetic, parsing or transforming text, counting things, converting \
     between formats, and the like — check whether a tool you already have (or could quickly \
     set up, e.g. installing a scripting language via os.execute_command the same way you'd \
     install anything else) would just do it faster and more reliably. If you've already shown \
     a capability works earlier in this same conversation, remember and reuse it rather than \
     defaulting back to manual work out of habit.",
    // "Before each tool call, briefly say (1-2 sentences, not a wall of reasoning) what you're \
    //  about to do, why, and what you expect the result to tell you. After the result comes \
    //  back, briefly note whether it matched that expectation before deciding the next step. \
    //  This applies every time, including partway through a long chain of tool calls in the \
    //  same turn — someone reading the conversation should be able to follow what you're doing \
    //  and why without reading your thinking.",
    "A background job (os.start_job) tells you when it finishes: a message appears in the chat \
     saying how it ended, and you get a turn to respond to it. So after starting one there's no \
     need to wait or poll — either carry on with other work, or end your turn saying what's \
     running and what you'll do once it's done. If you're unsure whether a job finished, \
     os.list_jobs says.",
    "Before attaching images, if you have capability, verify, that images shows exactly what you \
     wanted to show to the user. After making changes in code verify that they are actually \
     compiles and work as expected, if not asked otherwise",
    "Same applies to verification, not just changes: treat your first conclusion as a \
     hypothesis, not as verification. When you are about to make a factual claim about \
     something you can directly inspect, use the appropriate tool or source to verify it \
     first whenever one is available. If you're writing code and whether it works matters, \
     compile or run it; if you're analyzing an image, actually inspect the image; if you're \
     describing a file, read the file; if you're checking some external or mutable state, \
     query its current state. Do not substitute what you expect, remember, or infer for a \
     check that you can actually perform. Only say something was compiled, tested, run, \
     inspected, read, or otherwise verified after the relevant tool or source actually \
     established it. If verification is ambiguous, incomplete, or contradicts your initial \
     assumption, investigate further or state the uncertainty instead of confidently \
     presenting the assumption as fact.",
];

/// Pure boundary-selection for `Agent::compact` — pulled out of it so the arithmetic is
/// checkable on its own, without a live `ChatStore`/`OllamaService`. `sizes` is each
/// message's weight (content + thinking chars, plus each attached image's base64
/// length — a proportional stand-in for its real token cost, not an exact one, same
/// spirit as `KEEP_CHARS_PER_TOKEN` below), oldest first (same order `compact` reverses
/// its messages into). Walks from the newest (the end) backward, keeping a message only if it still
/// fits under `keep_chars` alongside everything newer already kept; returns the index
/// where `[0, index)` should be folded away and `[index, len)` kept verbatim. `0` means
/// nothing needs folding — everything already fits.
fn pick_compaction_boundary(sizes: &[usize], keep_chars: usize) -> usize {
    let mut kept_chars = 0usize;
    let mut split_at = sizes.len();

    for (index, &size) in sizes.iter().enumerate().rev() {
        if kept_chars + size > keep_chars {
            break;
        }
        kept_chars += size;
        split_at = index;
    }

    split_at
}

/// Prepends a short, bracketed fact about `file_ids` to `content` for whatever Ollama
/// actually sees — same convention `plugins::messaging::plugin` already uses for its
/// own per-message annotations (e.g. `[Message from user named ...]`). Unlike that
/// one, this is never baked into what `ChatStore` persists: the UI already shows
/// attached files as their own chips (`file_ids` on `MessageOut`), so repeating them as
/// ugly bracket text inside the message bubble would just be visual noise there — this
/// only ever runs on the copy of a message's content built for the actual `/api/chat`
/// request, at `to_ollama_message`'s history-replay call site and `Agent::chat`'s
/// fresh-turn one. A fact about *this specific message*, not a stable capability
/// claim, so it belongs here (rebuilt fresh every time a message is turned into what
/// Ollama sees, on every single replay) rather than in `SYSTEM_PROMPT` — see
/// `SYSTEM_PROMPT`'s own new rule for the paired behavioral instruction (use the tool
/// when it matters, don't guess). A no-op when there's nothing attached.
fn with_attached_files_note(content: String, file_ids: &[i64]) -> String {
    if file_ids.is_empty() {
        return content;
    }

    let ids = file_ids.iter().map(|id| format!("id {id}")).collect::<Vec<_>>().join(", ");
    format!(
        "[This message has file(s) attached: {ids}. You have NOT seen their content — this is \
         true even if this same message also shows you a real image: that image is a separate \
         thing from these file ids and tells you nothing about what's in them. Call \
         files.get_attached_file with one of these ids first if a file's actual content \
         matters for your answer.]\n{content}"
    )
}

/// Why a model reply was thrown away and asked for again instead of being stored.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplyProblem {
    /// The model wrote a tool call out as text (in its reasoning or its answer) instead
    /// of actually calling the tool, so nothing ran and the turn would just end there.
    ToolCallAsText,
    /// No answer, no tool call, and it wasn't cut off — nothing to show or act on.
    Empty,
    /// The model was cut off by the token limit strictly while still in its reasoning trace
    /// (empty content and no tool calls), leaving an incomplete and stalled reply.
    CutOffInThinking,
}

impl ReplyProblem {
    fn describe(self) -> &'static str {
        match self {
            ReplyProblem::ToolCallAsText => "tool call written out as text instead of being called",
            ReplyProblem::Empty => "empty reply (no answer, no tool call)",
            ReplyProblem::CutOffInThinking => "reply cut off by token limit while still in thinking",
        }
    }
}

/// How many times `advance` regenerates one reply before keeping whatever it got. A
/// malformed tool call is a sampling accident — asking again almost always fixes it —
/// so it gets two more tries. An empty reply can just as well be the right answer (the
/// user asked it to say nothing), which asking again will only repeat, so it gets one:
/// enough to catch a stall, cheap enough when it wasn't one. Cut off in thinking gets one retry
/// after emergency compaction so the model can reason to completion with fresh headroom.
#[derive(Default)]
struct Regenerations {
    tool_call_as_text: u8,
    empty: u8,
    cut_off_in_thinking: u8,
}

impl Regenerations {
    fn allow(&mut self, problem: ReplyProblem) -> bool {
        let (used, max) = match problem {
            ReplyProblem::ToolCallAsText => (&mut self.tool_call_as_text, 2),
            ReplyProblem::Empty => (&mut self.empty, 1),
            ReplyProblem::CutOffInThinking => (&mut self.cut_off_in_thinking, 1),
        };
        if *used >= max {
            return false;
        }
        *used += 1;
        true
    }
}

/// The text of the `notice` message written when a background job ends — what the model
/// is told, and what the chat shows. Bracketed like the other backend-written notes
/// (`with_attached_files_note`), and names the job by id and command so it's
/// recognisable without the model having to remember which job that was.
fn job_notice_text(job: &JobRecord) -> String {
    const MAX_COMMAND_CHARS: usize = 120;
    let command: String = job.command.chars().take(MAX_COMMAND_CHARS).collect();
    let ellipsis = if job.command.chars().count() > MAX_COMMAND_CHARS { "..." } else { "" };

    let outcome = match (job.status, job.exit_code) {
        (JobStatus::Exited, Some(0)) => "finished successfully (exit code 0)".to_string(),
        (JobStatus::Exited, Some(code)) => format!("exited with code {code}"),
        (JobStatus::Lost, _) => "was lost — the backend restarted while it was running, so how it \
                                 ended, or whether it's still running, is unknown"
            .to_string(),
        _ => "ended".to_string(),
    };

    format!(
        "[Background job {} (`{command}{ellipsis}`) {outcome}. Read its output with os.job_output.]",
        job.id
    )
}

/// Facade over `OllamaService`, `ChatStore`, and `ToolService` — where the actual
/// "fetch history, call Ollama, persist the result, run tool calls" sequencing lives,
/// rather than in route handlers or inside any one of the services it composes. Holds
/// its own `Arc` clones of each rather than borrowing from `AppState`, so it can be
/// used independently of any particular request's `State` extraction.
#[derive(Clone)]
pub struct Agent {
    ollama: Arc<OllamaService>,
    chat_store: Arc<ChatStore>,
    tools: Arc<ToolService>,
    /// Template `use_tool` calls `copy_with_chat_id` on to get the real, per-call
    /// context — its own `chat_id` is unused/meaningless (never itself handed to a
    /// tool). See `ToolContext`'s own doc comment for why it isn't `AppState`.
    tool_context: ToolContext,
    /// Where finished background jobs are looked up when a chat's next turn starts — see
    /// `flush_job_notices`. Also reachable to tools through `tool_context`.
    job_store: Arc<JobStore>,
    /// Per-chat tool-permission grants — what scope each tool has already been given
    /// within a given chat, if any. Consulted by `to_agent_tool_call`/`use_tool` to
    /// decide whether a call is `Allowed` outright or needs the caller to confirm.
    permission_store: Arc<PermissionStore>,
    /// How many of a chat's most recent messages to pull back for a single
    /// `chat`/`use_tool` call — both the conversation history sent to Ollama and the
    /// window `pending_tool_calls` scans backward through to find unresolved tool
    /// calls. A configuration knob rather than a constant for the same reason a model's
    /// context length is — how much history is worth paying for is a deployment
    /// decision, not something this code should hardcode.
    history_len: u64,
    /// See `TRIGGER_FRACTION` — `context_length * TRIGGER_FRACTION`, computed once
    /// here rather than at every `maybe_compact` call.
    compaction_trigger_tokens: u64,
    /// See `KEEP_CHARS_PER_TOKEN` — `context_length * KEEP_CHARS_PER_TOKEN`.
    compaction_keep_chars: usize,
    /// Chats with a tool call executing right now. See `RunningToolGuard`.
    running_tools: Arc<Mutex<HashSet<i64>>>,
}

/// What the model is told about a tool call that was cut short — worded to make it check what
/// state the call left instead of assuming either outcome, and to steer a command that never
/// exits toward `os.start_job`.
const INTERRUPTED_TOOL_MESSAGE: &str = "Interrupted — the backend stopped (or the connection dropped) before \
    this call finished, so it may have run only partly or not at all, and it was not run again \
    automatically. Check what state it left before repeating it. Anything that never exits on its own \
    (a dev server, a watcher) belongs in os.start_job, not a foreground command.";

/// Marks a chat as having a tool call executing, for as long as it lives. Dropping it — on
/// completion, on an error, or because the request was cancelled (a client that disconnects drops
/// the handler's future) — clears the mark. Without it a chat with an *allowed* call in flight is
/// indistinguishable from one whose call was cut short by a restart: both look like "allowed and
/// still unresolved".
struct RunningToolGuard {
    running: Arc<Mutex<HashSet<i64>>>,
    chat_id: i64,
}

impl Drop for RunningToolGuard {
    fn drop(&mut self) {
        self.running.lock().unwrap().remove(&self.chat_id);
    }
}

impl Agent {
    // One parameter per service the agent depends on — it's a wiring point, so it grows
    // with the services rather than with anything that would be clearer grouped.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ollama: Arc<OllamaService>,
        chat_store: Arc<ChatStore>,
        tools: Arc<ToolService>,
        file_store: Arc<FileStore>,
        job_store: Arc<JobStore>,
        events: Arc<EventBus>,
        permission_store: Arc<PermissionStore>,
        history_len: u64,
        context_length: u64,
    ) -> Self {
        // `chat_id: 0` here is a placeholder — never read as-is, always replaced via
        // `copy_with_chat_id` before a tool actually sees this context. `ollama` is
        // cloned (an `Arc` bump) rather than moved directly, since `Agent` itself also
        // holds its own copy below.
        let tool_context =
            ToolContext {
                file_store,
                ollama: ollama.clone(),
                job_store: job_store.clone(),
                events,
                chat_id: 0,
                user_id: 0,
                model: String::new(),
            };
        Self {
            ollama,
            chat_store,
            tools,
            tool_context,
            job_store,
            permission_store,
            history_len,
            compaction_trigger_tokens: (context_length as f64 * TRIGGER_FRACTION) as u64,
            compaction_keep_chars: (context_length as f64 * KEEP_CHARS_PER_TOKEN) as usize,
            running_tools: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Claims the chat's single tool-execution slot. Two calls running at once for one chat (a
    /// second tab opened, or a reload, while a long command is still going) would run the same
    /// pending call twice.
    fn mark_running(&self, chat_id: i64) -> Result<RunningToolGuard, ErrorService> {
        if !self.running_tools.lock().unwrap().insert(chat_id) {
            return Err(ErrorService::new(StatusCode::CONFLICT, "a tool call is already running for this chat"));
        }
        Ok(RunningToolGuard { running: self.running_tools.clone(), chat_id })
    }

    fn is_running(&self, chat_id: i64) -> bool {
        self.running_tools.lock().unwrap().contains(&chat_id)
    }

    /// Records the tool calls that were cut short as interrupted, instead of leaving them to be
    /// run again. Opening a chat is where this is discovered: a call the chat's grants already
    /// allow is executed the moment the model asks for it, never left waiting for a person — so
    /// one that is *still* unresolved while nothing is executing was interrupted (the backend
    /// restarted, or the client went away mid-run), and re-running it could repeat something that
    /// already happened, or block again on a command that never exits. A call waiting for the
    /// user's confirmation is different and stays as it is; so does everything queued after it,
    /// since results are recorded in the order the model asked.
    async fn settle_interrupted(&self, chat_id: i64) -> Result<(), ErrorService> {
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
                })
                .await?;
        }
        Ok(())
    }

    /// Persists `prompt` (plus `images`, if any — base64-encoded, no data-URL prefix —
    /// and `file_ids`, if any) as a `user` message, then advances the chat same as
    /// `continue_chat` does. The prompt is saved before the Ollama call, not after, so
    /// a failed/slow Ollama call never loses what the user actually sent. `think` is
    /// forwarded to Ollama as-is — see `OllamaService::chat` for its default.
    /// `file_ids` are only persisted here, never sent to Ollama or otherwise read —
    /// feeding a file's actual content into a turn is a separate, not-yet-built step —
    /// but each one does get claimed for this chat first (`FileStore::attach_to_chat`):
    /// a file can be uploaded before any chat exists to attach it to (the home page's
    /// case, `chat_id: None` until now), and this is the moment it actually becomes
    /// this chat's. A no-op for a file that was already uploaded with a real `chat_id`
    /// (e.g. from an existing chat's own composer) — this just re-sets it to the same
    /// value either way, cheaper than checking first.
    pub async fn chat(
        &self,
        chat_id: i64,
        prompt: String,
        images: Vec<String>,
        file_ids: Vec<i64>,
        think: Option<ThinkChoice>,
    ) -> Result<ChatOut, ErrorService> {
        let last_prompt_tokens = self.chat_store.chat(chat_id).await.ok().and_then(|c| c.last_prompt_tokens.map(|t| t as u64));
        self.maybe_compact(chat_id, last_prompt_tokens).await;
        let messages = self.ollama_history(chat_id).await?;

        for &file_id in &file_ids {
            self.tool_context.file_store.attach_to_chat(file_id, chat_id).await?;
        }

        self.chat_store
            .new_message(NewMessage {
                chat_id,
                role: "user".to_string(),
                content: prompt.clone(),
                tool_name: None,
                thinking: None,
                thought_duration_ms: None,
                tool_success: None,
                tool_denied: false,
                tool_calls: vec![],
                images: images.clone(),
                file_ids: file_ids.clone(),
                prompt_tokens: None,
                eval_tokens: None,
            })
            .await?;

        let notices = self.flush_job_notices(chat_id).await?;

        // Everything newer than `messages`, oldest first: the user's prompt, then any
        // job notices flushed just above (persisted after it, so this is also their
        // order in the chat). The last of them is what `advance` sends as the newest
        // message; the rest go in front of it as ordinary history.
        let ollama_content = with_attached_files_note(prompt, &file_ids);
        let mut tail = vec![OllamaService::user_message_with_images(ollama_content, images)];
        tail.extend(notices.iter().map(|notice| OllamaService::user_message(notice.content.clone())));
        let new_message = tail.pop();
        let mut messages = messages;
        messages.extend(tail);

        self.advance(chat_id, messages, new_message, think, notices, true).await
    }

    /// Sends a chat's existing history to Ollama as-is and persists whatever it replies
    /// with, without adding any new turn first. For continuing after tool results:
    /// `use_tool` already persisted the tool's output as a message, so getting the
    /// model's next response needs nothing more than asking again — no synthetic user
    /// message, no requirement that every pending tool call has been resolved first (a
    /// tool failing is a valid reason to continue too, and forcing the caller through the
    /// rest of an in-flight batch first would just be busywork).
    pub async fn continue_chat(&self, chat_id: i64, think: Option<ThinkChoice>) -> Result<ChatOut, ErrorService> {
        let last_prompt_tokens = self.chat_store.chat(chat_id).await.ok().and_then(|c| c.last_prompt_tokens.map(|t| t as u64));
        self.maybe_compact(chat_id, last_prompt_tokens).await;
        let notices = self.flush_job_notices(chat_id).await?;
        let messages = self.ollama_history(chat_id).await?;
        self.advance(chat_id, messages, None, think, notices, true).await
    }

    /// Starts a turn only if a background job has finished since the model was last
    /// told about one — `None` if there's nothing to report (or the chat is mid-turn, see
    /// below), so a caller acting on a stale hint (`ServerEvent::JobFinished` for a job an in-progress turn already
    /// reported) gets a harmless no-op instead of an unprompted extra model call. The
    /// decision is made here, not by the caller, because only here is claiming the
    /// finished jobs atomic. Otherwise identical to `continue_chat`: the notices are
    /// persisted as the newest messages and the model replies to them.
    pub async fn run_pending_notices(
        &self,
        chat_id: i64,
        think: Option<ThinkChoice>,
    ) -> Result<Option<ChatOut>, ErrorService> {
        // A notice has to come after every tool result already in the chat, never in the
        // middle of an unfinished batch — so while calls are still waiting to be run
        // (mid-turn, or paused on a confirmation) this does nothing, and the notice goes
        // out with the `continue_chat` that follows once they have.
        if !self.pending_tool_calls(chat_id).await?.is_empty() {
            return Ok(None);
        }

        let notices = self.flush_job_notices(chat_id).await?;
        if notices.is_empty() {
            return Ok(None);
        }

        let last_prompt_tokens = self.chat_store.chat(chat_id).await.ok().and_then(|c| c.last_prompt_tokens.map(|t| t as u64));
        self.maybe_compact(chat_id, last_prompt_tokens).await;
        let messages = self.ollama_history(chat_id).await?;
        Ok(Some(self.advance(chat_id, messages, None, think, notices, true).await?))
    }

    /// Turns every background job of `chat_id` that has finished but not been reported
    /// into a persisted `notice` message, oldest first, and returns them. Called at the
    /// one point in every turn where appending is always safe — after the newest
    /// message already stored, before the model's own reply — so a notice can never
    /// land between an assistant message's tool calls and their results. Persisted (not
    /// just added to the prompt) so the model keeps seeing it on later turns, the chat
    /// reads the same after a reload as it did live, and the reply that follows makes
    /// sense next to it. Each job is claimed before its notice is written, so
    /// concurrent callers can't report it twice; a job whose notice fails to save is
    /// handed back so the next turn tries again.
    async fn flush_job_notices(&self, chat_id: i64) -> Result<Vec<NoticeOut>, ErrorService> {
        let jobs = self.job_store.claim_unnotified(chat_id).await?;

        let mut notices = Vec::with_capacity(jobs.len());
        for job in jobs {
            let stored = self
                .chat_store
                .new_message(NewMessage {
                    chat_id,
                    role: "notice".to_string(),
                    content: job_notice_text(&job),
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
                })
                .await;

            match stored {
                Ok(message) => notices.push(NoticeOut { content: message.content, created_at: message.created_at }),
                Err(e) => {
                    if let Err(undo) = self.job_store.unclaim(job.id).await {
                        tracing::error!(job_id = job.id, "couldn't hand a job back after its notice failed to save: {undo}");
                    }
                    return Err(e.into());
                }
            }
        }

        Ok(notices)
    }

    /// Sends `messages` (plus `new_message`, if any) to Ollama, prefixed with
    /// `SYSTEM_PROMPT`, and persists whatever it replied with as an assistant message,
    /// carrying `tool_calls` if the model requested any. Shared by `chat` and
    /// `continue_chat`, which differ only in whether there's a new turn to add before
    /// asking the model to respond. `think` is forwarded to Ollama as-is (defaulted
    /// there, not here). `SYSTEM_PROMPT` is prepended fresh on every call rather than
    /// stored in `chat_store`, so it can be changed without touching existing chats'
    /// history. `thought_duration_ms` times the whole Ollama call, not just the
    /// `<think>` portion — see its doc comment on `NewMessage` for why — and includes any
    /// regenerations (`unusable_reply`), since that's how long the reply really took. `notices` are
    /// job notices the caller already persisted just before this call (see
    /// `flush_job_notices`), carried through into the returned `ChatOut` so a client can
    /// show them ahead of the reply.
    async fn advance(
        &self,
        chat_id: i64,
        mut messages: Vec<OllamaChatMessage>,
        new_message: Option<OllamaChatMessage>,
        think: Option<ThinkChoice>,
        notices: Vec<NoticeOut>,
        can_auto_continue: bool,
    ) -> Result<ChatOut, ErrorService> {
        let tools: Vec<&dyn Tool> = self.tools.get_tools().map(|tool| tool.as_ref()).collect();

        // The chat metadata, read fresh so a model switch takes effect on this very turn
        // and last_prompt_tokens is available for accurate budget calculation.
        let chat = self.chat_store.chat(chat_id).await?;
        let model = chat.model;
        let known_prompt_tokens = chat.last_prompt_tokens.map(|t| t as u64);

        // `ollama_history` leads with its own system message (the compaction summary)
        // once a chat has one — folded into this same system message rather than sent
        // as a second one, since some chat templates (e.g. Qwen's) reject more than one
        // system-role message anywhere but position 0 ("System message must be at the
        // beginning"). Deliberately holds no per-call-volatile content (no timestamp —
        // see `now_note` below for why) so it stays byte-identical across a chat's
        // turns except when a compaction fold actually changes the summary — that's
        // what lets Ollama/llama.cpp's prompt cache match this prefix and reuse it
        // instead of reprocessing the whole history on every single turn.
        let mut system_prompt = SYSTEM_PROMPT.join("\n");
        if messages.first().is_some_and(|message| message.role == "system") {
            let summary_message = messages.remove(0);
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&summary_message.content);
        }

        // Collected now, while `messages` still holds this turn's history, so it
        // survives the `extend` below. Only actually used once we know this response
        // has no further tool calls of its own — see the `stored` message below.
        let attached_files = Self::pending_attached_files(&messages);

        let mut messages_with_system = vec![OllamaService::system_message(system_prompt)];
        messages_with_system.extend(messages);

        // Stamped onto whichever message is newest — `new_message` when there is one
        // (`chat`'s fresh-turn path), otherwise the last entry already in
        // `messages_with_system` (`continue_chat`'s path, where that's the tool result
        // `use_tool` just persisted). Either way that's content this request sends to
        // Ollama for the first time, so appending it here doesn't cost any additional
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
        let new_message = match new_message {
            Some(mut msg) => {
                msg.content.push_str(&now_note);
                Some(msg)
            }
            None => {
                if let Some(last) = messages_with_system.last_mut() {
                    last.content.push_str(&now_note);
                }
                None
            }
        };

        let started_at = Instant::now();
        let mut regenerations = Regenerations::default();
        let response = loop {
            let response = self
                .ollama
                .chat(
                    messages_with_system.clone(),
                    new_message.clone(),
                    &tools,
                    think.clone(),
                    &model,
                    known_prompt_tokens,
                )
                .await?;

            let Some(problem) = self.unusable_reply(&response, &model).await else { break response };

            if problem == ReplyProblem::CutOffInThinking && can_auto_continue {
                let thought_trace = response
                    .message
                    .thinking
                    .as_deref()
                    .unwrap_or(&response.message.content)
                    .trim()
                    .to_string();
                let thought_duration_ms = i64::try_from(started_at.elapsed().as_millis()).unwrap_or(i64::MAX);
                let prompt_eval_count = response.prompt_eval_count();
                let eval_count = response.eval_count();

                tracing::warn!(
                    chat_id,
                    "model cut off in thinking; storing thought process as message and triggering automatic continuation"
                );

                // Store cut thoughts as message content (not as thinking) so they are replayed
                // back to the model on the continuation turn (`to_ollama_message` only sends `content`).
                let content = format!("[My thought process before being interrupted by token limit]:\n{thought_trace}");
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
                self.maybe_compact(chat_id, total_tokens).await;

                // Persist the continuation prompt into the chat store as a user message so the
                // database message sequence is strictly alternating (assistant -> user -> assistant),
                // preventing Ollama's "Cannot have 2 or more assistant messages at the end" error.
                let continuation_text = "[System note: Token limit reached during thinking. Based on your thoughts above, output your next response or tool call now.]".to_string();
                self.chat_store
                    .new_message(NewMessage {
                        chat_id,
                        role: "user".to_string(),
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
                    })
                    .await?;

                let fresh_messages = self.ollama_history(chat_id).await?;
                return Box::pin(self.advance(chat_id, fresh_messages, None, think, notices, false)).await;
            }

            if !regenerations.allow(problem) {
                tracing::warn!(chat_id, problem = problem.describe(), "model reply unusable, keeping it anyway");
                break response;
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
        };
        let thought_duration_ms = i64::try_from(started_at.elapsed().as_millis()).unwrap_or(i64::MAX);
        let prompt_eval_count = response.prompt_eval_count();
        let eval_count = response.eval_count();
        let prompt_tokens = prompt_eval_count.map(|c| c as i64);
        let eval_tokens = eval_count.map(|c| c as i64);

        let thinking = response.message.thinking.clone();

        let requested_tool_calls = response.message.tool_calls.unwrap_or_default();
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
                self.to_agent_tool_call(chat_id, call.function.name, call.function.arguments)
                    .await?,
            );
        }

        let out = ChatOut {
            content: stored.content,
            created_at: stored.created_at,
            can_use_tools: !tool_calls.is_empty(),
            tool_calls,
            thinking,
            thought_duration_ms,
            file_ids: stored.file_ids,
            notices,
        };

        let total_tokens = prompt_eval_count.map(|pt| pt + eval_count.unwrap_or(0));
        self.maybe_compact(chat_id, total_tokens).await;

        Ok(out)
    }

    /// Whether a model reply is unusable — see `ReplyProblem` — and so should be asked for
    /// again rather than stored. A reply that carries a real tool call is always usable,
    /// and so is one that was cut off by the token limit: asking again would just run
    /// into the same wall (`num_predict` already leaves all the room there is).
    ///
    /// Tool-call-as-text is recognized by the model's own wrapper tags (`<tool_call>` /
    /// `</tool_call>` for Qwen, whatever another model's template says — see
    /// `OllamaService::tool_call_markers`) turning up in its reasoning or answer with no
    /// call actually parsed. Emptiness alone deliberately doesn't count as that: it can be
    /// a legitimate reply, so it's its own, more cautious, problem.
    async fn unusable_reply(&self, response: &OllamaChatResponse, model: &str) -> Option<ReplyProblem> {
        let message = &response.message;
        if message.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()) {
            return None;
        }
        if response.done_reason.as_deref() == Some("length") {
            if message.content.trim().is_empty() {
                return Some(ReplyProblem::CutOffInThinking);
            }
            return None;
        }

        // The wrapper tags always contain "tool_call"/"function_call" (that's how
        // they're found), so text without either can't contain one — which keeps the
        // template lookup off the path of every ordinary reply.
        let texts = [message.thinking.as_deref().unwrap_or_default(), message.content.as_str()];
        if texts.iter().any(|text| text.contains("tool_call") || text.contains("function_call")) {
            let markers = self.ollama.tool_call_markers(model).await;
            if texts.iter().any(|text| markers.iter().any(|marker| text.contains(marker.as_str()))) {
                return Some(ReplyProblem::ToolCallAsText);
            }
        }

        message.content.trim().is_empty().then_some(ReplyProblem::Empty)
    }

    /// A chat's message history mapped into Ollama's wire format, oldest first. Once a
    /// chat has a compaction summary (`Chat::summary`/`summary_up_to_message_id` — see
    /// `compact`), that replaces everything up to the boundary as a single system
    /// message, prefixed with key facts (if any). Only what's newer is sent verbatim;
    /// otherwise this is the full history up to `history_len`, same as before compaction
    /// existed.
    async fn ollama_history(&self, chat_id: i64) -> Result<Vec<OllamaChatMessage>, ErrorService> {
        let chat = self.chat_store.chat(chat_id).await?;

        match (&chat.summary, chat.summary_up_to_message_id) {
            (Some(summary), Some(boundary_id)) => {
                let recent = self
                    .chat_store
                    .messages_after(chat_id, boundary_id, self.history_len)
                    .await?;

                let mut system_content = String::from(
                    "Earlier parts of this conversation were summarized to keep it within \
                     the model's context window."
                );

                // Prepend key facts (goal + list) if available. Facts are durable —
                // they persist across folds and don't get rewritten.
                if let Some(ref key_facts) = chat.key_facts {
                    system_content.push_str("\n\nKey facts (durable; still in effect unless a later message contradicts them):");
                    if let Some(ref goal) = key_facts.goal {
                        system_content.push_str(&format!("\nGoal: {goal}"));
                    }
                    for fact in &key_facts.facts {
                        system_content.push_str(&format!("\n- {fact}"));
                    }
                }

                system_content.push_str("\n\nSummary of everything before this point:\n\n");
                system_content.push_str(summary);

                let mut history = vec![OllamaService::system_message(system_content)];
                history.extend(recent.into_iter().rev().map(Self::to_ollama_message));
                Ok(history)
            }
            _ => {
                let (history, _) = self.chat_store.messages(chat_id, self.history_len, 0).await?;
                Ok(history.into_iter().rev().map(Self::to_ollama_message).collect())
            }
        }
    }

    /// Checks whether the turn that just finished (or last turn before starting a new one) pushed prompt usage over
    /// `compaction_trigger_tokens` and, if so, compacts older history into
    /// `Chat::summary` before returning — so the *next* request (a fresh turn, or
    /// another `continue_chat` later in the same tool-calling round) builds a smaller
    /// prompt via `ollama_history`. Best-effort: a failure here doesn't fail the turn
    /// that already succeeded, it just means history stays as big as it is and gets
    /// another chance to trigger this again.
    async fn maybe_compact(&self, chat_id: i64, prompt_eval_count: Option<u64>) {
        if prompt_eval_count.unwrap_or(0) < self.compaction_trigger_tokens {
            return;
        }

        tracing::info!(
            chat_id,
            prompt_eval_count,
            trigger_threshold = self.compaction_trigger_tokens,
            "compaction triggered for chat_id {chat_id}"
        );

        if let Err(e) = self.compact(chat_id).await {
            tracing::warn!(
                "history compaction failed for chat {chat_id}: {}",
                e.message.as_deref().unwrap_or("unknown error")
            );
        }
    }

    /// Folds the oldest not-yet-summarized messages into `Chat::summary` until what's
    /// left is under `compaction_keep_chars`, merging in the existing summary (if any)
    /// rather than discarding it. No-ops if everything already fits — that means
    /// `compaction_trigger_tokens` fired on a single outsized turn rather than a long
    /// history, which folding can't help with.
    async fn compact(&self, chat_id: i64) -> Result<(), ErrorService> {
        let chat = self.chat_store.chat(chat_id).await?;
        let after_id = chat.summary_up_to_message_id.unwrap_or(0);

        let mut messages = self
            .chat_store
            .messages_after(chat_id, after_id, self.history_len)
            .await?;
        messages.reverse(); // oldest first, easier to reason about a boundary over

        let sizes: Vec<usize> = messages
            .iter()
            .map(|message| {
                message.content.len()
                    + message.thinking.as_deref().map_or(0, str::len)
                    + message.images.iter().map(String::len).sum::<usize>()
            })
            .collect();
        let split_at = pick_compaction_boundary(&sizes, self.compaction_keep_chars);

        if split_at == 0 {
            return Ok(());
        }

        let to_fold = &messages[..split_at];
        let new_boundary_id = to_fold.last().map(|m| m.id).unwrap_or(after_id);

        tracing::info!(
            chat_id,
            messages_folded = to_fold.len(),
            had_prior_summary = chat.summary.is_some(),
            "calling summarize for chat_id {chat_id}"
        );

        let summary = self.summarize(chat.summary.clone(), to_fold, &chat.model).await?;
        let facts = self.extract_facts(to_fold, chat.key_facts.clone(), &chat.model).await;
        let existing_facts_count = chat.key_facts.as_ref().map_or(0, |f| f.facts.len());
        let merged = Self::merge_facts(chat.key_facts.clone().unwrap_or_default(), facts.goal, facts.facts);
        // `merge_facts` only ever appends, so this can't underflow in practice — saturating
        // anyway rather than trusting that invariant never breaks in the future.
        let facts_added = merged.facts.len().saturating_sub(existing_facts_count);

        self.chat_store
            .set_summary(chat_id, summary, merged, new_boundary_id)
            .await?;

        tracing::info!(chat_id, new_boundary_id, facts_added, "compaction finished for chat_id {chat_id}");

        Ok(())
    }

    /// Produces an updated summary covering `existing_summary` (if any) plus every
    /// message in `to_fold`, via a plain (no tools) Ollama call — not part of the
    /// visible conversation, so it doesn't go through `advance`/get persisted as a chat
    /// message itself.
    async fn summarize(
        &self,
        existing_summary: Option<String>,
        to_fold: &[Message],
        model: &str,
    ) -> Result<String, ErrorService> {
        // The image data itself never goes into the transcript (it's not text, and this
        // call carries no vision guarantee) — but a message that had one needs to say
        // so, or folding it away loses any trace it ever happened, silently.
        let transcript = to_fold
            .iter()
            .map(|message| match message.role.as_str() {
                "tool" => format!(
                    "[tool result — {}]: {}",
                    message.tool_name.as_deref().unwrap_or("?"),
                    message.content
                ),
                role if !message.images.is_empty() => format!(
                    "[{role}, {} image(s) attached]: {}",
                    message.images.len(),
                    message.content
                ),
                role => format!("[{role}]: {}", message.content),
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prior = existing_summary
            .map(|summary| format!("Summary of everything before this excerpt:\n{summary}\n\n"))
            .unwrap_or_default();

        let system = OllamaService::system_message(
            "Summarize the conversation excerpt that follows into short but complete notes \
             for continuing the conversation later — what was asked, what was done, what was \
             learned, and any specific details (file paths, decisions, exact names) later \
             turns might still need. If a prior summary is included, fold it in rather than \
             dropping it. Pay special attention to detail that's easy to accidentally \
             paraphrase away but matters a lot if lost: a tool result marked truncated (a \
             later turn needs to know it only saw part of something, not the whole thing), \
             exact code/text snippets that a future edit might need to reproduce verbatim \
             — summarize the surrounding narrative, but don't rewrite exact text like that \
             into your own words — and a message marked as having image(s) attached (the \
             images themselves aren't in this excerpt, only that mark — keep noting that \
             one was there, since a later turn may still need to know an image was part of \
             what was asked). Write plain notes, not a reply — this output \
             replaces the excerpt in the conversation's history, nobody sees it directly. \
             Record only what's actually stated or shown in the excerpt — never add your own \
             suggested next steps, recommendations, or assumptions about what should happen \
             next; a later turn will decide that itself from the real conversation, and an \
             invented 'next step' can send it chasing something nobody actually needs. If \
             something in the excerpt looks contradictory or doesn't add up (e.g. a filename \
             or detail that doesn't match elsewhere), note the discrepancy plainly rather \
             than inventing an explanation that resolves it — a guessed resolution that's \
             wrong is worse than an acknowledged gap.\n\n\
             Exact facts (paths, names+versions, confirmed API idioms, decisions, constraints, \
             config, open items) are tracked in a separate structured list; don't reproduce \
             them exhaustively in the summary — focus on what was asked, done, and learned."
                .to_string(),
        );
        let user = OllamaService::user_message(format!(
            "{prior}Conversation excerpt to summarize:\n\n{transcript}"
        ));

        // `think: false` — measured head-to-head against the same real fold-candidate
        // messages (`summarize_bench`, since deleted): thinking cost ~2x the time and
        // token budget, and produced a *shorter, less detailed* final summary — the
        // deliberation ate the token budget that would've otherwise gone into exact
        // struct/field names and per-tool specifics, which is exactly what the system
        // prompt above asks it to preserve. Reasoning first turned out to hurt the
        // thing it was meant to help here, not just cost more.
        let response = self
            .ollama
            .chat(vec![system], Some(user), &[], Some(ThinkChoice::Enabled(false)), model, None)
            .await?;
        Ok(response.message.content)
    }

    /// Returns a plain-text transcript of messages suitable for both summarize and
    /// extract-facts prompts — same role annotations and image/mark conventions as
    /// summarize's own transcript, factored out so the code isn't duplicated.
    fn transcript_of(to_fold: &[Message]) -> String {
        to_fold
            .iter()
            .map(|message| match message.role.as_str() {
                "tool" => format!(
                    "[tool result — {}]: {}",
                    message.tool_name.as_deref().unwrap_or("?"),
                    message.content
                ),
                role if !message.images.is_empty() => format!(
                    "[{role}, {} image(s) attached]: {}",
                    message.images.len(),
                    message.content
                ),
                role => format!("[{role}]: {}", message.content),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Extracts new key facts from the fold, producing a `ChatFacts` with optional
    /// `goal` (only if existing goal is absent) and a list of new facts.
    ///
    /// Best-effort: on parse failure or Ollama error, logs a warning and falls back
    /// to the existing facts (cloned, or `ChatFacts::default()` if none). This means
    /// the fold still proceeds even when fact extraction fails — the model just
    /// doesn't get better-structured context on the next fold until extraction works.
    async fn extract_facts(
        &self,
        to_fold: &[Message],
        existing_key_facts: Option<ChatFacts>,
        model: &str,
    ) -> ChatFacts {
        let existing = existing_key_facts.as_ref();
        let existing_goal = existing.and_then(|f| f.goal.clone());
        let existing_facts_str = existing.map_or("none".into(), |f| {
            if f.facts.is_empty() {
                "none".into()
            } else {
                format!(
                    "\nExisting facts (do not repeat):\n{}",
                    f.facts
                        .iter()
                        .map(|fact| format!("- {}", fact))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
        });

        let transcript = Self::transcript_of(to_fold);

        let system = OllamaService::system_message(format!(
            "Extract key facts from the conversation excerpt. Output ONLY a JSON object: \
             {{\"goal\": <string|null>, \"facts\": [<string>]}}. No prose, no markdown, no \
             code fences.\n\
             \n\
             Rules:\n\
             - goal: the user's core request for the whole chat (one sentence, \
             present-tense). Only set if the existing goal is absent/NULL. Never modify an \
             existing goal. If the chat has no clear goal or the existing goal is already \
             set, send null.\n\
             - facts: a short list of new, specific facts extracted from this excerpt. Only \
             include facts not already in the existing list. Deduplicate yourself. Skip \
             empty or whitespace-only results.\n\
             \n\
             What is a fact:\n\
             - Exact paths, names, versions, identifiers (e.g., \
             'backend/src/services/chat_store.rs')\n\
             - Confirmed API idioms and tool-call patterns (e.g., 'Ollama /api/chat with \
             tool_calls: []')\n\
             - Design decisions and constraints ('key_facts is JSONB, nullable, persisted \
             via set_summary')\n\
             - Configuration, feature flags, environment variables\n\
             - Open or unresolved items that a later turn might need to know about\n\
             \n\
             What is NOT a fact:\n\
             - Narrative descriptions of what happened\n\
             - Next steps, recommendations, or suggestions\n\
             - Vague or generic observations\n\
             \n\
             Verbatim strings for paths, identifiers, and API signatures. One sentence per \
             fact. If nothing new: {{\"goal\": null, \"facts\": []}}\n\
             {existing_facts_str}"
        ));

        let user = OllamaService::user_message(format!(
            "Extract new key facts from the conversation excerpt below.

Existing goal: {}\n\nConversation excerpt:\n\n{transcript}",
            existing_goal.as_deref().unwrap_or("(none)"),
        ));

        match self.ollama.chat(vec![system], Some(user), &[], Some(ThinkChoice::Enabled(false)), model, None).await {
            Err(err) => {
                let es: ErrorService = err.into();
                tracing::warn!(
                    error = es.message.as_deref().unwrap_or("unknown error"),
                    "fact extraction ollama call failed, using existing facts"
                );
                ChatFacts {
                    goal: existing_goal,
                    facts: existing.map_or(vec![], |f| f.facts.clone()),
                }
            }
            Ok(response) => {
                let raw = response.message.content;

                match Self::parse_extracted_facts(&raw) {
                    Ok((trimmed_goal, new_facts)) => {
                        if trimmed_goal.is_none() && existing_goal.is_some() {
                            tracing::debug!("extracted goal was empty/none, keeping existing");
                        }

                        if new_facts.is_empty() && trimmed_goal.is_none() {
                            tracing::debug!("extracted facts empty");
                        } else {
                            tracing::info!(
                                facts_extracted = new_facts.len(),
                                "successfully extracted facts from fold"
                            );
                        }

                        ChatFacts {
                            goal: trimmed_goal,
                            facts: new_facts,
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            raw = %raw,
                            "fact extraction parse failed, using existing facts"
                        );
                        ChatFacts {
                            goal: existing_goal,
                            facts: existing.map_or(vec![], |f| f.facts.clone()),
                        }
                    }
                }
            }
        }
    }

    /// Strips a leading/trailing ```` ```json ```` or ```` ``` ```` fence if the model wrapped
    /// its output in one, otherwise returns the trimmed input unchanged.
    fn strip_json_fences(raw: &str) -> &str {
        raw.strip_prefix("```json")
            .or_else(|| raw.strip_prefix("```"))
            .map(|s| s.trim_start().strip_suffix("```").map(|s| s.trim()).unwrap_or(s.trim()))
            .unwrap_or(raw.trim())
    }

    /// Parses `extract_facts`'s raw Ollama response into `(goal, facts)`, tolerating an
    /// optional code fence around the JSON. Trims and drops empty entries so callers never
    /// see whitespace-only facts or an empty-string goal. A pure function (no I/O, no
    /// `self`) so it's unit-testable without a live Ollama call — see the tests below.
    fn parse_extracted_facts(raw: &str) -> Result<(Option<String>, Vec<String>), serde_json::Error> {
        #[derive(Deserialize)]
        struct WireFacts {
            goal: Option<String>,
            facts: Vec<String>,
        }

        let cleaned = Self::strip_json_fences(raw);
        let wire: WireFacts = serde_json::from_str(cleaned)?;

        let goal = wire
            .goal
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_string);

        let facts = wire
            .facts
            .into_iter()
            .map(|f| f.trim().to_string())
            .filter(|f| !f.is_empty())
            .collect();

        Ok((goal, facts))
    }


    /// The tool calls the model has asked for that haven't been run yet, without
    /// actually running them — lets a caller check each one's `permission` (and warn
    /// about a `Denied` one) before committing to `use_tool`.
    ///
    /// Also settles calls that were cut short (see `settle_interrupted`): they're recorded as
    /// interrupted rather than reported as pending, so opening a chat never re-runs them. While a
    /// call is executing, nothing is reported as pending — whoever is running it owns it.
    pub async fn can_use_tool(&self, chat_id: i64) -> Result<CanUseTool, ErrorService> {
        if self.is_running(chat_id) {
            return Ok(CanUseTool { can_use: false, tools: vec![] });
        }
        self.settle_interrupted(chat_id).await?;

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

    /// Runs the next pending tool call (in the order the model requested them) and
    /// persists its result as a `tool`-role message, whether it succeeded, failed, or
    /// was denied — the model needs to see all three outcomes to react sensibly on its
    /// next turn, not just a silent gap. `scope`, if given, overrides whatever's
    /// already stored for this chat/tool for this one call only — it's never
    /// persisted (`allow_scope` is the only thing that persists a grant) — and doubles
    /// as the caller's confirmation that it knows what it's asking for. With no
    /// override, whatever's already stored (if anything) is used instead.
    ///
    /// A call the tool doesn't permit — `ToolPermission::Denied`, whether from no
    /// scope being available at all or from a given/stored one not covering it — never
    /// reaches `call_tool`. It's recorded the same way an execution failure is
    /// (`success: false`, persisted as the `tool` message), but with `denied: true` so
    /// a caller can tell "this needs permission" apart from "this tool actually broke"
    /// without parsing `err`'s text.
    ///
    /// Errors only when there's nothing pending to run, which means the caller didn't
    /// check `can_use_tool` first. Also reports the tool calls still left after this one,
    /// same as `can_use_tool` would, so a caller can tell whether to run another `use_tool`
    /// or move on without a separate round trip.
    pub async fn use_tool(&self, chat_id: i64, scope: Option<Value>) -> Result<UseToolOut, ErrorService> {
        let _running = self.mark_running(chat_id)?;
        let mut pending = self.pending_tool_calls(chat_id).await?.into_iter();
        let next = pending
            .next()
            .ok_or_else(|| ErrorService::new(StatusCode::BAD_REQUEST, "no pending tool call to run"))?;

        let had_scope;
        let effective_scope = match scope {
            Some(scope) => {
                had_scope = true;
                match self.tools.get_tool(&next.tool_name) {
                    Some(tool) => resolved_scope_from_json(tool, scope),
                    None => ResolvedScope::default(),
                }
            }
            None => {
                let stored = self.stored_scope(chat_id, &next.tool_name).await?;
                had_scope = stored.own.is_some() || !stored.shared.is_empty();
                stored
            }
        };

        let permission = self.tool_permission(&next.tool_name, next.arguments.clone(), effective_scope);

        let (success, denied, err, content) = match permission {
            AgentToolPermission::Allowed => {
                let chat = self.chat_store.chat(chat_id).await?;
                let ctx = self.tool_context.copy_with_chat_id(chat_id, chat.user_id, chat.model);
                match self.tools.call_tool(&next.tool_name, next.arguments, &ctx).await {
                    Ok(value) => (true, false, None, value),
                    Err(e) => {
                        let message = e.to_string();
                        (false, false, Some(message.clone()), Value::String(message))
                    }
                }
            }
            AgentToolPermission::Denied { reason, escalation } => {
                // Worded so the model doesn't read one declined call as a ban on the
                // tool as a whole — it's scoped to this specific call, and retrying
                // (same arguments once the user grants it, or different arguments
                // that aren't restricted) is the expected next step, not something to
                // refuse on principle.
                let message = match (had_scope, escalation.is_some()) {
                    (_, false) => format!(
                        "Tool call blocked — '{}' can't be approved for these exact arguments ({reason}). \
                         This only concerns this specific call, not the tool as a whole.",
                        next.tool_name
                    ),
                    (true, true) => format!(
                        "Tool call denied — the permission already granted doesn't cover these arguments \
                         ({reason}). Call it again with arguments the user's willing to approve, or let them \
                         decide."
                    ),
                    (false, true) => format!(
                        "Tool call declined — '{}' hasn't been granted permission in this chat yet ({reason}). \
                         If the user wants to proceed, call it again; they'll be asked to approve it then.",
                        next.tool_name
                    ),
                };
                (false, true, Some(message.clone()), Value::String(message))
            }
        };

        let stored = self
            .chat_store
            .new_message(NewMessage {
                chat_id,
                role: "tool".to_string(),
                content: content.to_string(),
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
            })
            .await?;

        let mut tools = Vec::with_capacity(pending.len());
        for call in pending {
            tools.push(self.to_agent_tool_call(chat_id, call.tool_name, call.arguments).await?);
        }

        Ok(UseToolOut {
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
    pub async fn allow_scope(&self, chat_id: i64, tool_name: String, scope: Value) -> Result<(), ErrorService> {
        let Some(tool) = self.tools.get_tool(&tool_name) else {
            return Err(ErrorService::new(
                StatusCode::BAD_REQUEST,
                format!("no tool named '{tool_name}'"),
            ));
        };

        let delta = resolved_scope_from_json(tool, scope);

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
        let Some(tool) = self.tools.get_tool(tool_name) else {
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
    fn tool_permission(&self, tool_name: &str, data: Value, scope: ResolvedScope) -> AgentToolPermission {
        let Some(tool) = self.tools.get_tool(tool_name) else {
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
    async fn pending_tool_calls(&self, chat_id: i64) -> Result<Vec<ToolCallOut>, ErrorService> {
        let (messages, _) = self.chat_store.messages(chat_id, self.history_len, 0).await?;

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
    async fn to_agent_tool_call(
        &self,
        chat_id: i64,
        name: String,
        arguments: Value,
    ) -> Result<AgentToolCall, ErrorService> {
        let scope = self.stored_scope(chat_id, &name).await?;
        let permission = self.tool_permission(&name, arguments.clone(), scope);

        Ok(AgentToolCall {
            permission,
            name,
            arguments,
        })
    }

    /// Scans back through this turn's already-loaded history for `ui.attach_file`
    /// results, collecting their `file_id`s. Stops at the last `user`-role message
    /// (or the start of history), since that's where the current turn began — any
    /// `tool`/`assistant` messages before it belong to an earlier turn, whose own
    /// attach results were already attached to *that* turn's final reply when it was
    /// generated. Everything from here to the end of history is this turn's own
    /// tool-calling rounds (`assistant` messages requesting tools, `tool` messages
    /// with their results), so no other role needs to stop the scan.
    fn pending_attached_files(messages: &[OllamaChatMessage]) -> Vec<i64> {
        let attach_file_name = AttachFileTool.function_name();
        let mut file_ids: Vec<i64> = messages
            .iter()
            .rev()
            .take_while(|message| message.role != "user")
            .filter(|message| message.role == "tool" && message.tool_name.as_deref() == Some(attach_file_name))
            .filter_map(|message| serde_json::from_str::<Value>(&message.content).ok())
            .filter_map(|value| value.get("file_id").and_then(Value::as_i64))
            .collect();
        file_ids.reverse();
        file_ids
    }

    /// Maps a persisted `Message` back into the shape Ollama's `/api/chat` expects for
    /// history. `OllamaToolCall::id` is left empty — we never persisted Ollama's
    /// original per-call id (only `function.name`/`arguments`, which is all replaying
    /// history needs), and it's not yet confirmed whether Ollama expects/uses `id` at
    /// all on the *outgoing* (request) side versus just returning it in responses.
    fn to_ollama_message(message: Message) -> OllamaChatMessage {
        let tool_calls: Vec<OllamaToolCall> = message
            .tool_calls
            .into_iter()
            .enumerate()
            .map(|(index, call)| OllamaToolCall {
                id: String::new(),
                function: OllamaToolCallFunction {
                    index: Some(index as u32),
                    name: call.tool_name,
                    arguments: call.arguments,
                },
            })
            .collect();

        OllamaChatMessage {
            // A `notice` is the backend telling the model something (a job finished) —
            // chat templates only know system/user/assistant/tool, and it reads as
            // something said to the model, so it goes out as a `user` message.
            role: if message.role == "notice" { "user".to_string() } else { message.role },
            content: with_attached_files_note(message.content, &message.file_ids),
            tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
            tool_name: message.tool_name,
            thinking: None,
            images: (!message.images.is_empty()).then_some(message.images),
        }
    }

    /// Deterministic merge of existing facts with a fresh extraction.
    ///
    /// - goal: keep existing.goal if set and non-empty, else take new_goal (if non-empty).
    /// - facts: push each new fact, skipping empty/whitespace-only ones and any whose
    ///   trimmed form case-insensitively equals an existing entry's trimmed form.
    /// - Never reorder, never rewrite, never drop existing entries.
    ///
    /// This is a pure function so it's unit-testable and independent of any service
    /// wiring — correctness-critical invariants are enforced here, not by the model.
    fn merge_facts(existing: ChatFacts, new_goal: Option<String>, new_facts: Vec<String>) -> ChatFacts {
        let goal = existing.goal.or_else(|| {
            let trimmed = new_goal?.trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        });

        let mut facts = existing.facts;
        for new_fact in new_facts {
            let trimmed = new_fact.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            let exists = facts.iter().any(|existing_fact| {
                existing_fact.trim().to_lowercase() == trimmed.to_lowercase()
            });
            if !exists {
                facts.push(trimmed);
            }
        }

        ChatFacts { goal, facts }
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

/// `Agent`'s outputs (this one included) derive `Serialize` and go straight out as JSON
/// from route handlers, unlike service-layer types — `Agent` *is* the app's public API
/// shape already (that's what a facade is), so there's nothing upstream-specific left to
/// strip before a route can return it, unlike `OllamaChatMessage` et al.
#[derive(Serialize, ToSchema)]
pub struct AgentToolCall {
    pub permission: AgentToolPermission,
    pub name: String,
    #[schema(value_type = Object)]
    pub arguments: Value,
}

/// Facade-owned mirror of `tools::base::ToolPermission` — kept as a separate type
/// (rather than deriving `Serialize`/`ToSchema` on the original and reusing it
/// directly) for the same reason `OllamaChatMessage` never goes straight out over our
/// API: the tools layer's internal shape shouldn't be what callers of `Agent` end up
/// depending on.
#[derive(Serialize, ToSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentToolPermission {
    Allowed,
    Denied {
        reason: String,
        escalation: Option<AgentScopeGrant>,
    },
}

/// Facade-owned mirror of `tools::base::ScopeGrant` — see `AgentToolPermission` for
/// why this isn't the original type reused directly.
#[derive(Serialize, ToSchema)]
pub struct AgentScopeGrant {
    #[schema(value_type = Object)]
    pub scope: Value,
    pub ui_message: String,
}

#[derive(Serialize, ToSchema)]
pub struct CanUseTool {
    pub can_use: bool,
    pub tools: Vec<AgentToolCall>,
}

#[derive(Serialize, ToSchema)]
pub struct ChatOut {
    pub content: String,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
    pub can_use_tools: bool,
    pub tool_calls: Vec<AgentToolCall>,
    /// The model's reasoning trace for this reply, when `think` was requested and the
    /// model produced one.
    pub thinking: Option<String>,
    /// How long the Ollama call for this reply took, in milliseconds — see
    /// `NewMessage::thought_duration_ms` for what this does and doesn't measure. Always
    /// set (unlike the same-named field on `Message`/`NewMessage`) — `advance` times
    /// every call it makes, there's no path through it that skips this.
    pub thought_duration_ms: i64,
    /// Mirrors `NewMessage::file_ids` for this reply — a `ui.attach_file` call earlier
    /// in the same turn, resolved onto this (the turn's final, non-tool-calling) reply.
    /// Without this, a freshly-arrived reply couldn't show its attachment until the
    /// chat was reloaded from `GET /chats/messages`, which is the only other place
    /// `file_ids` comes from.
    pub file_ids: Vec<i64>,
    /// Background-job notices persisted just before this reply, oldest first — messages
    /// the client should show in the chat ahead of `content`, in this order. Empty
    /// unless a job finished since the previous turn.
    pub notices: Vec<NoticeOut>,
}

/// One `notice` message: the backend telling the chat a background job ended.
#[derive(Serialize, ToSchema)]
pub struct NoticeOut {
    pub content: String,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
}

#[derive(Serialize, ToSchema)]
pub struct UseToolOut {
    pub success: bool,
    /// Set when `success` is `false` specifically because the call wasn't permitted
    /// (no/insufficient scope) — distinct from `success: false` with `denied: false`,
    /// which means the tool ran and genuinely failed. Lets a caller (e.g. the UI) tell
    /// the two apart without matching on `err`'s text.
    pub denied: bool,
    pub tool_name: String,
    pub err: Option<String>,
    /// What the tool itself returned — the same value persisted as the `tool` message's
    /// content. On failure this is the error message wrapped as a JSON string, same as
    /// what got persisted.
    #[schema(value_type = Object)]
    pub content: Value,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
    pub tools: Vec<AgentToolCall>,
}
