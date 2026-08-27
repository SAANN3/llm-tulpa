use std::sync::atomic::{AtomicI64, Ordering};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::Sender;
use tool_derive::ToolParams;

use super::chunking::{split_top_level_blocks, strip_code_fence};
use super::provider::{IncomingMessage, MessagingProvider};
use crate::plugins::base::PluginError;
use crate::tools::base::{PropertyInfo, PropertyType, ToolParams};

#[derive(Debug, Clone, Deserialize, Serialize, ToolParams)]
pub struct VkSettings {
    #[tool(description = "The community's access token, from Group settings → API usage → Access tokens.")]
    pub token: String,
    #[tool(description = "The community (group) numeric ID this bot belongs to.")]
    pub group_id: i64,
}

/// The API version sent on every call — deliberately NOT VK's publicly documented
/// "latest" (5.199), which caps `messages.sendReaction`'s `reaction_id` range at 1-16.
/// VK's own official client actually talks to 5.255 (undocumented — found by a third
/// party inspecting the client's own network traffic, not from any VK doc page),
/// where that range extends to 1-64. This matters directly for `VK_EYES_REACTION_ID`
/// below (32, outside 5.199's range) — pinned by hand regardless of that, since VK
/// breaks response shapes across versions independently of anything in this crate.
const VK_API_VERSION: &str = "5.255";
const VK_API_BASE: &str = "https://api.vk.com/method";

/// VK's own documented cap on a single `messages.send` text is 4096 characters. This
/// budget stays conservatively under that, same reasoning as Telegram's/Discord's own
/// per-platform constants.
const VK_MAX_MESSAGE_CHARS: usize = 4000;

/// Peer ids at or above this belong to a multi-user chat (`2_000_000_000 +
/// local_chat_id`, VK's own documented convention) rather than a direct one-on-one
/// conversation with a user — the threshold every VK bot library uses to tell the two
/// apart, since nothing in a message's own fields states it directly.
const CHAT_PEER_ID_THRESHOLD: i64 = 2_000_000_000;

/// Sent back — as a reply — instead of forwarding anything to the agent when `/chat`
/// (or any other `/command`) arrives with nothing after it, same reasoning as
/// Telegram's/Discord's own `BARE_COMMAND_PROMPT`.
const BARE_COMMAND_PROMPT: &str = "What would you like to ask?(Reply to this message)";

/// VK's `reaction_id` for 👀 (eyes) — the original reading, taken straight from the
/// VK client app's own `messages.sendReaction` request body when reacting with 👀 for
/// real. It only looked wrong earlier because this provider was pinned to API version
/// 5.199 (see `VK_API_VERSION`), whose `reaction_id` range tops out at 16 — 32 was
/// rejected as "invalid" purely for being out of that range, not because it was ever
/// the wrong id — confirmed live with `VK_API_VERSION` at 5.255 (whose range covers
/// it), 👀 now renders correctly. The only id this provider ever needs:
/// `RECEIVED_REACTION` in `plugin.rs` is always "👀", nothing else is ever passed to
/// `react_on_message` here.
const VK_EYES_REACTION_ID: i64 = 32;

/// Splits a leading `/command` (or `/command arg`) syntax off the front of `text`, if
/// present — same shape and reasoning as Telegram's/Discord's own `split_command`.
fn split_command(text: &str) -> (bool, &str) {
    if !text.starts_with('/') {
        return (false, text);
    }

    match text.split_once(char::is_whitespace) {
        Some((_command, rest)) => (true, rest.trim_start()),
        None => (true, ""),
    }
}

/// Splits `text` into chunks under `VK_MAX_MESSAGE_CHARS`, breaking only at the
/// top-level blank-line boundaries `split_top_level_blocks` finds — never inside a
/// fenced code block. Like Discord (and unlike Telegram), this runs on the raw agent
/// reply with no escaping pass first: VK's message API never rejects a message over
/// stray punctuation — a `**bold**` marker just shows up literally rather than being
/// rendered or rejected — so there's nothing here that needs Telegram's
/// escape-then-split care.
fn split_for_vk(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for block in split_top_level_blocks(text) {
        for piece in split_block(block, VK_MAX_MESSAGE_CHARS) {
            let separator_len = if current.is_empty() { 0 } else { 2 };
            if !current.is_empty() && current.len() + separator_len + piece.len() > VK_MAX_MESSAGE_CHARS {
                chunks.push(std::mem::take(&mut current));
            }

            if !current.is_empty() {
                current.push_str("\n\n");
            }
            current.push_str(&piece);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Splits one top-level block down to `max_chars` pieces, no-op if it's already small
/// enough. A block that's a single self-contained fenced code block is re-fenced per
/// piece (see `wrap_code_fence`) rather than just hard-cut, same reasoning as
/// Telegram's/Discord's own `split_block`.
fn split_block(block: &str, max_chars: usize) -> Vec<String> {
    if block.len() <= max_chars {
        return vec![block.to_string()];
    }

    if let Some((lang, content)) = strip_code_fence(block) {
        return wrap_code_fence(lang, content, max_chars);
    }

    hard_split(block, max_chars).into_iter().map(str::to_string).collect()
}

/// Splits an oversized fenced code block's `content` into `max_chars`-sized pieces,
/// each wrapped in its own complete fence — mirrors Discord's `wrap_code_fence`: no
/// escape-pair concern, since `content` here is the model's raw text, never escaped.
fn wrap_code_fence(lang: &str, content: &str, max_chars: usize) -> Vec<String> {
    let opener = format!("```{lang}\n");
    let closer = "```";
    let budget = max_chars.saturating_sub(opener.len() + closer.len()).max(1);

    hard_split(content, budget)
        .into_iter()
        .map(|piece| format!("{opener}{piece}{closer}"))
        .collect()
}

/// Plain hard character-boundary split, only if `block` alone exceeds `max_chars` — no
/// escape-pair awareness needed, unlike Telegram's own `hard_split`.
fn hard_split(block: &str, max_chars: usize) -> Vec<&str> {
    if block.len() <= max_chars {
        return vec![block];
    }

    let mut pieces = Vec::new();
    let mut rest = block;
    while rest.len() > max_chars {
        let split_at = rest
            .char_indices()
            .take_while(|(i, _)| *i < max_chars)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(rest.len());

        let (head, tail) = rest.split_at(split_at);
        pieces.push(head);
        rest = tail;
    }
    if !rest.is_empty() {
        pieces.push(rest);
    }
    pieces
}

/// One raw call against `https://api.vk.com/method/<method>` — `access_token` and `v`
/// are appended here rather than by each caller, since every VK method this provider
/// uses needs both. VK's own error convention is a `200 OK` HTTP response whose JSON
/// body is `{"error": {...}}` instead of `{"response": ...}` — there's no HTTP-status
/// signal to check, the body has to be inspected either way.
async fn vk_call<T: DeserializeOwned>(
    client: &reqwest::Client,
    method: &str,
    token: &str,
    mut params: Vec<(&str, String)>,
) -> Result<T, PluginError> {
    params.push(("access_token", token.to_string()));
    params.push(("v", VK_API_VERSION.to_string()));

    let body: Value = client
        .post(format!("{VK_API_BASE}/{method}"))
        .form(&params)
        .send()
        .await
        .map_err(|e| PluginError::FailedUnknown(format!("vk {method} request failed: {e}")))?
        .json()
        .await
        .map_err(|e| PluginError::FailedUnknown(format!("vk {method} response parse failed: {e}")))?;

    if let Some(error) = body.get("error") {
        let code = error.get("error_code").and_then(Value::as_i64).unwrap_or(-1);
        let message = error.get("error_msg").and_then(Value::as_str).unwrap_or("unknown error");
        return Err(PluginError::FailedUnknown(format!("vk {method} api error {code}: {message}")));
    }

    let response = body
        .get("response")
        .cloned()
        .ok_or_else(|| PluginError::FailedUnknown(format!("vk {method} response missing both 'response' and 'error'")))?;

    serde_json::from_value(response).map_err(|e| PluginError::FailedUnknown(format!("vk {method} response shape mismatch: {e}")))
}

#[derive(Debug, Clone, Deserialize)]
struct LongPollServer {
    key: String,
    server: String,
    ts: String,
}

#[derive(Debug, Deserialize)]
struct LongPollCheckResponse {
    #[serde(default)]
    failed: Option<i64>,
    ts: Option<String>,
    #[serde(default)]
    updates: Vec<Value>,
}

pub struct VkProvider {
    token: String,
    group_id: i64,
    client: reqwest::Client,
    /// Our own counter for `messages.send`'s required `random_id` param (see
    /// `send_chunks`) — unrelated to the long-poll `key`/`ts` VK hands back from
    /// `groups.getLongPollServer`. Seeded from the current time so it starts unique
    /// across restarts too, not just within one.
    next_random_id: AtomicI64,
}

#[async_trait]
impl MessagingProvider for VkProvider {
    type Settings = VkSettings;

    fn subname() -> &'static str {
        "vk"
    }

    fn settings_schema() -> Vec<PropertyInfo> {
        VkSettings::tool_properties()
    }

    fn help_message() -> String {
        "How to use the VK plugin:\n\
         \n\
         1. Create a VK community (or reuse an existing one) at vk.com/groups → Create community.\n\
         2. In the community's Management → API usage → Access tokens, create a token with both the \"Messages\" and \"Manage community\" access rights checked — \"Messages\" alone isn't enough, groups.getLongPollServer (used to actually receive messages) needs \"Manage community\" too — and copy it.\n\
         3. Paste that token into this plugin's \"token\" setting, and the community's numeric ID (shown in Management → Info, or in the community's own URL) into \"group_id\", then save.\n\
         4. In Management → Messages, make sure community messages are enabled; in Management → Bots settings, turn on the Bots Long Poll API.\n\
         5. Still under Bots settings → Long Poll API → Settings → Event types, check \"Message received\" under Messages, and save — without this, message_new events are never delivered even though everything else is configured correctly.\n\
         6. Enable the plugin with the toggle.\n\
         7. Open the community's page and send it a direct message. Your own VK numeric user id — the chat id for a direct conversation — is visible on your own profile page's URL (vk.com/id<number>).\n\
         8. Add that id to \"allowed_chat_ids\" and save — the bot only ever responds in chats listed there; a message from anywhere else is just logged and ignored.\n\
         9. In a direct message, just send your message. In a group chat the bot's been added to, send /chat followed by your message (e.g. \"/chat what's the weather like\"), or reply to one of the bot's own messages.\n\
         10. The bot reacts with 👀 to show it received your message, then replies threaded to it once the answer is ready."
            .to_string()
    }

    async fn connect(settings: Self::Settings) -> Result<Self, PluginError> {
        Ok(Self {
            token: settings.token,
            group_id: settings.group_id,
            client: reqwest::Client::new(),
            next_random_id: AtomicI64::new(chrono::Utc::now().timestamp_millis()),
        })
    }

    /// Long-polls VK's Bots Long Poll API — `groups.getLongPollServer` once up front,
    /// then repeated `a_check` requests against the server it returns, exactly the
    /// pattern every VK bot library uses. `failed` in a check response is VK's own
    /// documented recovery signal, not a hard error: `1` means a few events may have
    /// been skipped but the connection itself is fine (the response's own `ts` says
    /// where to resume); `2` means the `key` alone expired (refetch it, but keep the
    /// locally-tracked `ts` — the freshly returned one isn't guaranteed contiguous);
    /// `3` means both `key` and `ts` are invalid (refetch both). Only an unrecognized
    /// code, or a request/parse failure, is treated as a real error worth bubbling up
    /// to `run_with_reconnect`.
    async fn run(&self, tx: Sender<IncomingMessage>) -> Result<(), PluginError> {
        let mut longpoll = self.fetch_longpoll_server(None).await?;
        let mut logged_connected = false;

        loop {
            let params = vec![
                ("act", "a_check".to_string()),
                ("key", longpoll.key.clone()),
                ("ts", longpoll.ts.clone()),
                ("wait", "25".to_string()),
            ];

            let response: LongPollCheckResponse = self
                .client
                .post(&longpoll.server)
                .form(&params)
                .send()
                .await
                .map_err(|e| PluginError::FailedUnknown(format!("vk long poll request failed: {e}")))?
                .json()
                .await
                .map_err(|e| PluginError::FailedUnknown(format!("vk long poll response parse failed: {e}")))?;

            if !logged_connected {
                tracing::info!("vk provider connected, long-polling for updates");
                logged_connected = true;
            }

            match response.failed {
                None => {
                    if let Some(ts) = response.ts {
                        longpoll.ts = ts;
                    }
                }
                Some(1) => {
                    if let Some(ts) = response.ts {
                        longpoll.ts = ts;
                    }
                    continue;
                }
                Some(2) => {
                    longpoll = self.fetch_longpoll_server(Some(longpoll.ts.clone())).await?;
                    continue;
                }
                Some(3) => {
                    longpoll = self.fetch_longpoll_server(None).await?;
                    continue;
                }
                Some(other) => {
                    return Err(PluginError::FailedUnknown(format!("vk long poll failed with unrecognized code {other}")));
                }
            }

            for update in response.updates {
                if update.get("type").and_then(Value::as_str) != Some("message_new") {
                    continue;
                }
                let Some(message) = update.get("object").and_then(|object| object.get("message")) else { continue };
                let Some(text) = message.get("text").and_then(Value::as_str) else { continue };
                let Some(peer_id) = message.get("peer_id").and_then(Value::as_i64) else { continue };
                let Some(cmid) = message.get("conversation_message_id").and_then(Value::as_i64) else { continue };
                let Some(from_id) = message.get("from_id").and_then(Value::as_i64) else { continue };

                let text = text.trim();
                let (is_command, after_command) = split_command(text);
                let is_direct = peer_id < CHAT_PEER_ID_THRESHOLD;
                let is_reply_to_bot = message
                    .get("reply_message")
                    .and_then(|reply| reply.get("from_id"))
                    .and_then(Value::as_i64)
                    .is_some_and(|reply_from_id| reply_from_id == -self.group_id);

                // VK, unlike Telegram, has no privacy-mode equivalent that filters
                // group-chat messages before they ever reach the bot — every message
                // in a chat the bot's a member of arrives here regardless of content.
                // A direct conversation is always addressed to the bot (nothing else
                // it could be); a group chat only counts a `/command` or a reply to
                // one of the bot's own messages as addressed, same filtering Discord
                // does by hand for the same reason.
                if !is_direct && !is_command && !is_reply_to_bot {
                    continue;
                }

                if is_command && after_command.is_empty() {
                    if let Err(err) = self.send_chunks(&peer_id.to_string(), BARE_COMMAND_PROMPT, None).await {
                        tracing::warn!("vk: failed to send bare-command prompt in peer {peer_id}: {err}");
                    }
                    continue;
                }

                let content = if is_command { after_command } else { text }.trim();
                if content.is_empty() {
                    continue;
                }

                let author = self.resolve_author(from_id).await;
                let incoming =
                    IncomingMessage { chat_id: peer_id.to_string(), message_id: cmid.to_string(), author, text: content.to_string() };

                if tx.send(incoming).await.is_err() {
                    return Ok(());
                }
            }
        }
    }

    async fn send_message(&self, chat_id: &str, text: &str) -> Result<(), PluginError> {
        self.send_chunks(chat_id, text, None).await
    }

    async fn reply_message(&self, chat_id: &str, message_id: &str, text: &str) -> Result<(), PluginError> {
        let cmid: i64 =
            message_id.parse().map_err(|_| PluginError::FailedUnknown(format!("invalid vk conversation_message_id: {message_id}")))?;
        self.send_chunks(chat_id, text, Some(cmid)).await
    }

    /// VK's `messages.sendReaction` only accepts one of its own fixed enum of
    /// `reaction_id`s, not an arbitrary emoji string — `RECEIVED_REACTION` in
    /// `plugin.rs` is always "👀" in practice, so that's the one case mapped to a real
    /// id (`VK_EYES_REACTION_ID`); anything else falls through to a no-op, same as the
    /// trait's own default, since there's no honest mapping for it.
    async fn react_on_message(&self, chat_id: &str, message_id: &str, reaction: &str) -> Result<(), PluginError> {
        if reaction != "👀" {
            return Ok(());
        }

        let cmid: i64 =
            message_id.parse().map_err(|_| PluginError::FailedUnknown(format!("invalid vk conversation_message_id: {message_id}")))?;

        let params = vec![
            ("peer_id", chat_id.to_string()),
            ("cmid", cmid.to_string()),
            ("reaction_id", VK_EYES_REACTION_ID.to_string()),
        ];
        vk_call::<Value>(&self.client, "messages.sendReaction", &self.token, params).await?;

        Ok(())
    }
}

impl VkProvider {
    /// Shared by `send_message` and `reply_message` — everything about getting `text`
    /// onto the chat except whether it's addressed to a prior message. `reply_to`
    /// (when given) is only attached to the *first* chunk, same reasoning as
    /// Telegram's/Discord's own `send_chunks`.
    ///
    /// Threading a reply uses the `forward` JSON param (`{peer_id,
    /// conversation_message_ids: [cmid], is_reply: 1}`) rather than `messages.send`'s
    /// own `reply_to` param — confirmed live: `reply_to` set to the incoming message's
    /// `cmid` (the same id `react_on_message` already uses successfully as
    /// `messages.sendReaction`'s `cmid`, so the id itself is valid) still failed with
    /// "One of the parameters specified was missing or invalid: forwarded message not
    /// found". Real-world reports from other VK bot libraries describe the same
    /// `reply_to`-from-a-bot unreliability and use this same `forward`-as-reply
    /// workaround, which is what several bot frameworks reach for by default.
    async fn send_chunks(&self, chat_id: &str, text: &str, reply_to: Option<i64>) -> Result<(), PluginError> {
        for (index, chunk) in split_for_vk(text).into_iter().enumerate() {
            let random_id = self.next_random_id.fetch_add(1, Ordering::Relaxed);
            let mut params =
                vec![("peer_id", chat_id.to_string()), ("message", chunk), ("random_id", random_id.to_string())];
            if index == 0 && let Some(cmid) = reply_to {
                let peer_id: i64 =
                    chat_id.parse().map_err(|_| PluginError::FailedUnknown(format!("invalid vk peer id: {chat_id}")))?;
                let forward = serde_json::json!({
                    "peer_id": peer_id,
                    "conversation_message_ids": [cmid],
                    "is_reply": 1,
                });
                params.push(("forward", forward.to_string()));
            }

            vk_call::<Value>(&self.client, "messages.send", &self.token, params).await?;
        }

        Ok(())
    }

    /// Fetches a fresh long-poll server via `groups.getLongPollServer`. `preserve_ts`,
    /// when given, overrides the freshly-returned `ts` with the caller's own — used on
    /// a `failed: 2` recovery (see `run`), where only the `key` is known to be stale
    /// and the locally-tracked `ts` is still the right place to resume from.
    async fn fetch_longpoll_server(&self, preserve_ts: Option<String>) -> Result<LongPollServer, PluginError> {
        let params = vec![("group_id", self.group_id.to_string())];
        let mut server: LongPollServer = vk_call(&self.client, "groups.getLongPollServer", &self.token, params).await?;
        if let Some(ts) = preserve_ts {
            server.ts = ts;
        }
        Ok(server)
    }

    /// Resolves `from_id` to a display name for `IncomingMessage::author` — unlike
    /// Telegram/Discord, VK's `message_new` payload carries only the numeric id, never
    /// a name, so this costs its own `users.get` call per incoming message. A negative
    /// id is VK's own convention for "posted as a community" rather than a person
    /// (mainly relevant if a community ever posts into a chat this bot also sees);
    /// `users.get` doesn't resolve those, so that case is handled locally instead.
    /// Falls back to the bare numeric id on any lookup failure — a less friendly but
    /// still usable label, not a reason to drop the message.
    async fn resolve_author(&self, from_id: i64) -> String {
        if from_id < 0 {
            return format!("community {}", -from_id);
        }

        let params = vec![("user_ids", from_id.to_string())];
        match vk_call::<Vec<Value>>(&self.client, "users.get", &self.token, params).await {
            Ok(users) => users
                .first()
                .and_then(|user| {
                    let first = user.get("first_name").and_then(Value::as_str)?;
                    let last = user.get("last_name").and_then(Value::as_str)?;
                    Some(format!("{first} {last}"))
                })
                .unwrap_or_else(|| from_id.to_string()),
            Err(_) => from_id.to_string(),
        }
    }
}
