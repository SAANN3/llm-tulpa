use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use teloxide_core::Bot;
use teloxide_core::net::Download;
use teloxide_core::requests::Requester;
use teloxide_core::types::{BotCommand, ChatId, ForceReply, MessageId, ParseMode, PhotoSize, ReactionType, ReplyParameters, UpdateKind};
use tokio::sync::Mutex;
use tokio::sync::mpsc::Sender;
use tool_derive::ToolParams;

use super::chunking::{split_top_level_blocks, strip_code_fence};
use super::markdown::to_telegram_markdown_v2;
use super::provider::{Attachment, IncomingMessage, MessagingProvider};
use crate::plugins::base::PluginError;
use crate::tools::base::{PropertyInfo, PropertyType, ToolParams};

#[derive(Debug, Clone, Deserialize, Serialize, ToolParams)]
pub struct TelegramSettings {
    #[tool(description = "The bot token from @BotFather.")]
    pub token: String,
}

/// How long Telegram is asked to hold a `getUpdates` long-poll open before returning
/// empty when there's nothing new — the standard long-polling pattern (a request only
/// returns early once an update actually arrives). The HTTP client's own request
/// timeout (see `connect`) has to be comfortably longer than this: otherwise the
/// client aborts the connection before Telegram's long-poll gets the chance to return
/// empty on its own, which surfaces (misleadingly) as a generic "network error" on
/// every single idle poll cycle rather than the client-side timeout it actually is.
const LONG_POLL_TIMEOUT_SECS: u32 = 30;

/// Telegram delivers a multi-photo album (`media_group_id`) as several independent
/// `Update`s in quick succession — its Bot API never marks which one is last, so the
/// only way to know an album is complete is to wait for a gap in arrivals. Every major
/// Telegram bot framework (aiogram, telegraf, python-telegram-bot) handles albums with
/// exactly this debounce-and-flush technique for the same reason: there's no other
/// signal to key off. Comfortably above the sub-second gaps Telegram normally leaves
/// between an album's parts, small enough that a real reply still feels prompt.
const MEDIA_GROUP_DEBOUNCE: Duration = Duration::from_millis(1200);

/// One in-progress album, keyed by `media_group_id` in `TelegramProvider::media_groups`
/// — accumulates each part's image as it arrives until the debounce task (spawned when
/// the group is first seen, see `run`) decides no more are coming and flushes it as a
/// single `IncomingMessage`. `chat_id`/`message_id`/`author`/`author_id` are fixed from
/// the first part seen — Telegram doesn't vary these across one album, and the first
/// part's message id is as valid a reply-thread anchor as any other part's.
struct MediaGroupBuffer {
    chat_id: String,
    message_id: String,
    author: String,
    author_id: String,
    sent_at: chrono::DateTime<chrono::Utc>,
    /// Only one part of a real album ever carries the caption; whichever part supplies
    /// non-empty text wins, since the rest simply have none to offer.
    text: String,
    attachments: Vec<Attachment>,
    last_seen: Instant,
}

/// Telegram rejects any single `sendMessage` over 4096 UTF-16 code units. This budget
/// stays conservatively under that in plain byte count — Cyrillic/CJK/emoji-heavy text
/// can diverge slightly from a UTF-16 unit count, so exact-limit math isn't worth it
/// for the headroom a flat conservative number already buys.
const TELEGRAM_MAX_MESSAGE_CHARS: usize = 3500;

/// Splits `text` — already converted by `to_telegram_markdown_v2` — into chunks under
/// `TELEGRAM_MAX_MESSAGE_CHARS`, breaking only at the top-level blank-line boundaries
/// `split_top_level_blocks` finds (paragraphs, code blocks, lists, …) — never inside a
/// fenced code block, so a block's own opening and closing ` ``` ` always end up
/// together. A single block still over the limit on its own is handed to `split_block`,
/// which re-fences a code block's own pieces rather than just hard-cutting through it.
fn split_for_telegram(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for block in split_top_level_blocks(text) {
        for piece in split_block(block, TELEGRAM_MAX_MESSAGE_CHARS) {
            let separator_len = if current.is_empty() { 0 } else { 2 };
            if !current.is_empty() && current.len() + separator_len + piece.len() > TELEGRAM_MAX_MESSAGE_CHARS {
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

/// Splits one top-level block (from `split_top_level_blocks`) down to `max_chars`
/// pieces, no-op if it's already small enough. A block that's a single self-contained
/// fenced code block (see `strip_code_fence`) is re-fenced per piece via
/// `split_code_fence` rather than just hard-cut — a bare `hard_split` would slice
/// straight through the ` ``` ` markers, leaving a piece with an opener and no closer
/// (or vice versa), which is what "Can't find end of Pre(Code) entity" means: Telegram
/// saw an unclosed fence because the piece it got was never a complete block on its
/// own. Anything else falls back to plain `hard_split`.
fn split_block(block: &str, max_chars: usize) -> Vec<String> {
    if block.len() <= max_chars {
        return vec![block.to_string()];
    }

    if let Some((lang, content)) = strip_code_fence(block) {
        return split_code_fence(lang, content, max_chars);
    }

    hard_split(block, max_chars).into_iter().map(str::to_string).collect()
}

/// Splits an oversized fenced code block's `content` into `max_chars`-sized pieces,
/// each wrapped in its own complete ` ```<lang> ` … ` ``` ` fence — so every resulting
/// Telegram message chunk is independently valid Markdown instead of a fragment of one
/// shared fence. `content` was escaped by `escape_code` (only backtick/backslash), the
/// same single-character-escape shape as MarkdownV2 text, so `hard_split`'s
/// escape-pair-aware cut applies here too.
fn split_code_fence(lang: &str, content: &str, max_chars: usize) -> Vec<String> {
    let opener = format!("```{lang}\n");
    // No inserted newline before the closer — matches `to_telegram_markdown_v2` itself
    // (`out.push_str("```")` runs right after the code content, no separator of its
    // own), which relies on the content's last line already ending in `\n` the way
    // fenced code content from `pulldown_cmark` normally does. Not forcing one here
    // keeps each piece an exact substring of `content` plus fixed fencing, so nothing
    // is added or dropped at a split point.
    let closer = "```";
    let budget = max_chars.saturating_sub(opener.len() + closer.len()).max(1);

    hard_split(content, budget)
        .into_iter()
        .map(|piece| format!("{opener}{piece}{closer}"))
        .collect()
}

/// Hard character-boundary split, only if `block` alone exceeds `max_chars` — the
/// last-resort fallback `split_block` reaches for once a whole top-level block (or a
/// single code-fence piece) doesn't fit under the limit even by itself.
///
/// `block` here is already MarkdownV2-escaped (`to_telegram_markdown_v2` ran first), so
/// every literal special character is preceded by a `\`. A naive char-count cut can
/// land between that `\` and the character it escapes, leaving the next piece starting
/// with a bare special character Telegram then rejects (e.g. "Character '(' is
/// reserved and must be escaped") — every `\` this module ever emits is one half of
/// such a pair (never standalone), so an odd number of trailing `\`s right at the cut
/// means it's about to split one; walking the cut back one character keeps the pair
/// together in the next piece instead.
fn hard_split(block: &str, max_chars: usize) -> Vec<&str> {
    if block.len() <= max_chars {
        return vec![block];
    }

    let mut pieces = Vec::new();
    let mut rest = block;
    while rest.len() > max_chars {
        let mut split_at = rest
            .char_indices()
            .take_while(|(i, _)| *i < max_chars)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(rest.len());

        let trailing_backslashes = rest[..split_at].chars().rev().take_while(|&c| c == '\\').count();
        if trailing_backslashes % 2 == 1 {
            split_at -= 1;
        }

        let (head, tail) = rest.split_at(split_at);
        pieces.push(head);
        rest = tail;
    }
    if !rest.is_empty() {
        pieces.push(rest);
    }
    pieces
}

/// Sent back — with a `ForceReply` attached (see the bare-command branch in `run`) —
/// instead of forwarding anything to the agent when `/chat` (or any other
/// `/command[@botname]`) arrives with nothing after it — what Telegram sends the
/// instant someone taps a command straight out of its own autocomplete menu, with no
/// chance to type a real message first. Forwarding that empty text to the agent as if
/// it were a real message just confuses it (and, since the same bare text repeats on
/// every retap, reads as spam), so this is the actual usage hint the command's
/// `set_my_commands` description alone can't fully convey.
const BARE_COMMAND_PROMPT: &str = "What would you like to ask?(Reply to this message)";

/// Splits Telegram's own `/command` or `/command@botname` syntax off the front of
/// `text`, if present — `(true, "")` for a bare command with nothing after it (see
/// `BARE_COMMAND_HINT`), `(true, "the rest")` for a command with a real message
/// attached, `(false, text)` unchanged for anything that isn't a command at all
/// (privacy mode also forwards plain @mentions and replies-to-the-bot, neither of
/// which start with `/`). The agent should never see the literal command syntax
/// itself — just the actual message, same as if it'd been said directly with no
/// command in front.
fn split_command(text: &str) -> (bool, &str) {
    if !text.starts_with('/') {
        return (false, text);
    }

    match text.split_once(char::is_whitespace) {
        Some((_command, rest)) => (true, rest.trim_start()),
        None => (true, ""),
    }
}

pub struct TelegramProvider {
    bot: Bot,
    /// Where the next `getUpdates` poll resumes from — Telegram's `offset` param.
    /// `None` until the first successful poll this provider instance makes; at that
    /// point it's bootstrapped via `latest_offset` to skip whatever backlog piled up
    /// before now (a process restart, or the plugin having sat disabled for a while)
    /// instead of replaying every message sent while nothing was listening. Persists
    /// across `run()` invocations — both reconnects and disable/enable cycles within
    /// the same process — so those resume from the real last-seen point instead of
    /// re-bootstrapping and skipping messages that arrived in between.
    offset: Mutex<Option<i32>>,
    /// In-progress albums, keyed by `media_group_id`. `Arc`-wrapped (independent of
    /// `self`, which `run`'s spawned per-group debounce task can't borrow — that task
    /// has to be `'static`) so it can be cloned into that task without cloning the
    /// whole provider.
    media_groups: Arc<Mutex<HashMap<String, MediaGroupBuffer>>>,
}

#[async_trait]
impl MessagingProvider for TelegramProvider {
    type Settings = TelegramSettings;

    fn subname() -> &'static str {
        "telegram"
    }

    fn settings_schema() -> Vec<PropertyInfo> {
        TelegramSettings::tool_properties()
    }

    fn help_message() -> String {
        "How to use the Telegram plugin:\n\
         \n\
         1. Message @BotFather on Telegram and create a bot (or reuse an existing one) to get a bot token.\n\
         2. Paste that token into this plugin's \"token\" setting and save.\n\
         3. Open a chat with your bot (search its @username) and send it any message — Telegram will show your numeric chat id in the message the bot receives, or you can get it from @userinfobot.\n\
         4. Add that chat id to \"allowed_chat_ids\" and save — the bot only ever responds in chats listed there.\n\
         5. Enable the plugin with the toggle.\n\
         6. In the chat, send /chat followed by your message (e.g. \"/chat what's the weather like\"), or just send /chat alone and reply to its prompt with your message.\n\
         7. The bot reacts with 👀 to show it received your message, then replies threaded to it once the answer is ready.\n\
         \n\
         In a group chat, the bot only sees messages sent as a /chat command, or messages that reply directly to one of the bot's own messages."
            .to_string()
    }

    async fn connect(settings: Self::Settings) -> Result<Self, PluginError> {
        // `Bot::new` builds a plain reqwest client that, on a network advertising a
        // dead IPv6 route for api.telegram.org (a real, fairly common dual-stack
        // misconfiguration — not specific to any one machine), can fail every request
        // instead of falling back to the working IPv4 address. Forcing the client's
        // local address to IPv4 sidesteps that: a socket bound to an IPv4 local address
        // can only ever connect to an IPv4 remote, so this client never attempts the
        // broken route in the first place. Otherwise identical to `Bot::new`'s own
        // client (same settings, via the same `default_reqwest_settings`).
        let client = teloxide_core::net::default_reqwest_settings()
            .local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            // `default_reqwest_settings()`'s own request timeout (17s) is shorter than
            // `LONG_POLL_TIMEOUT_SECS` (30s) — see that constant's doc comment for what
            // goes wrong if this isn't overridden.
            .timeout(Duration::from_secs(u64::from(LONG_POLL_TIMEOUT_SECS) + 10))
            .build()
            .map_err(|e| PluginError::FailedUnknown(format!("failed to build telegram http client: {e}")))?;

        Ok(Self {
            bot: Bot::with_client(settings.token, client),
            offset: Mutex::new(None),
            media_groups: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Long-polls `getUpdates`, tracking `offset` so already-delivered updates aren't
    /// re-fetched (Telegram only stops returning an update once an offset past it is
    /// requested) — the standard Bot API polling pattern, done by hand here rather
    /// than through teloxide's own `Dispatcher`, since that's built around its own
    /// `dptree` handler tree instead of just handing events to a channel.
    async fn run(&self, tx: Sender<IncomingMessage>) -> Result<(), PluginError> {
        let mut offset = {
            let mut offset = self.offset.lock().await;
            match *offset {
                Some(resume_from) => resume_from,
                None => {
                    let skip_to = Self::latest_offset(&self.bot).await?;
                    *offset = Some(skip_to);

                    // Also runs only on this same "first time this provider instance
                    // has ever started listening" path — registers `/chat` in
                    // Telegram's own command menu (the autocomplete list shown when
                    // typing "/"), purely as a discoverability hint. It's not required
                    // for `/chat` (or any other slash-command) to actually reach the
                    // bot — Telegram's privacy mode already forwards *any* message
                    // starting with a recognized `/command` syntax regardless of
                    // registration — this just makes that fact visible to someone who
                    // wouldn't otherwise know a bare mention isn't enough in a group.
                    if let Err(err) = self
                        .bot
                        .set_my_commands(vec![BotCommand::new("chat", "Talk to the assistant — type your message after the command")])
                        .await
                    {
                        tracing::warn!("telegram set_my_commands failed (non-fatal): {err}");
                    }

                    skip_to
                }
            }
        };

        // Logged once per `run()` invocation (so once per fresh connect *and* once per
        // successful reconnect), right after the first `getUpdates` call actually
        // succeeds — distinct from `MessagingPlugin::on_enabled`'s own "enabled" log,
        // which only confirms the background tasks were spawned, not that this loop
        // ever reached Telegram at all.
        let mut logged_connected = false;

        loop {
            let mut request = self.bot.get_updates();
            request.offset = Some(offset);
            request.timeout = Some(LONG_POLL_TIMEOUT_SECS);

            let updates = request
                .await
                .map_err(|e| PluginError::FailedUnknown(format!("telegram get_updates failed: {e}")))?;

            if !logged_connected {
                tracing::info!("telegram provider connected, long-polling for updates");
                logged_connected = true;
            }

            for update in updates {
                offset = update.id.as_offset();
                *self.offset.lock().await = Some(offset);

                let UpdateKind::Message(message) = update.kind else { continue };
                // A photo message has no `text()` (that's `None` for anything but a
                // plain text message) — its caption, if any, is the closest equivalent.
                // `photo()` is checked separately so a photo with no caption at all
                // still gets through, rather than being treated as if nothing arrived.
                let text = message.text().or_else(|| message.caption()).unwrap_or("");
                let photo = message.photo();
                if text.is_empty() && photo.is_none() {
                    continue;
                }
                let chat_id = message.chat.id.0.to_string();

                let (is_command, content) = split_command(text);
                if is_command && content.is_empty() && photo.is_none() {
                    // `ForceReply` opens the reply UI on the *sender's* client, prefilled
                    // to reply to this exact prompt — their next message then arrives as
                    // a normal update with `reply_to_message` set, no different from any
                    // other message we already process. `.selective()` restricts the
                    // forced UI to whoever actually sent the bare command (via
                    // `reply_parameters` below marking this prompt as a reply to their
                    // message) instead of nudging every member of a group chat.
                    let mut prompt = self.bot.send_message(message.chat.id, BARE_COMMAND_PROMPT);
                    prompt.reply_parameters = Some(ReplyParameters { message_id: message.id, ..Default::default() });
                    prompt.reply_markup = Some(
                        ForceReply::new()
                            .input_field_placeholder("Type your message…".to_string())
                            .selective()
                            .into(),
                    );

                    if let Err(err) = prompt.await {
                        tracing::warn!("telegram: failed to send bare-command prompt to {chat_id}: {err}");
                    }
                    continue;
                }
                let content = if is_command { content } else { text };

                let attachments = match photo {
                    Some(sizes) => match self.download_largest_photo(sizes).await {
                        Ok(image) => vec![Attachment::Image(image)],
                        Err(err) => {
                            tracing::warn!("telegram: failed to download photo in chat {chat_id}: {err}");
                            vec![]
                        }
                    },
                    None => vec![],
                };

                let author = message.from.as_ref().map(|user| user.full_name()).unwrap_or_else(|| "unknown".to_string());
                let author_id = message.from.as_ref().map(|user| user.id.0.to_string()).unwrap_or_else(|| "unknown".to_string());

                // A multi-photo album: buffer this part instead of sending it on its
                // own — see `MEDIA_GROUP_DEBOUNCE` for why a wait is unavoidable here.
                if let Some(group_id) = message.media_group_id() {
                    let group_id = group_id.0.clone();
                    let mut groups = self.media_groups.lock().await;
                    let is_new_group = !groups.contains_key(&group_id);
                    let buffer = groups.entry(group_id.clone()).or_insert_with(|| MediaGroupBuffer {
                        chat_id: chat_id.clone(),
                        message_id: message.id.0.to_string(),
                        author: author.clone(),
                        author_id: author_id.clone(),
                        sent_at: message.date,
                        text: String::new(),
                        attachments: Vec::new(),
                        last_seen: Instant::now(),
                    });
                    if !content.is_empty() {
                        buffer.text = content.to_string();
                    }
                    buffer.attachments.extend(attachments);
                    buffer.last_seen = Instant::now();
                    drop(groups);

                    // One debounce task per album, spawned only when its first part
                    // arrives — every later part just updates `last_seen` above and
                    // lets the already-running task pick the change up.
                    if is_new_group {
                        let media_groups = self.media_groups.clone();
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            loop {
                                tokio::time::sleep(MEDIA_GROUP_DEBOUNCE).await;
                                let mut groups = media_groups.lock().await;
                                let Some(buffer) = groups.get(&group_id) else { return };
                                if buffer.last_seen.elapsed() < MEDIA_GROUP_DEBOUNCE {
                                    // A newer part arrived mid-sleep — wait another round.
                                    continue;
                                }
                                let buffer = groups.remove(&group_id).expect("just confirmed present above");
                                drop(groups);

                                let incoming = IncomingMessage {
                                    chat_id: buffer.chat_id,
                                    message_id: buffer.message_id,
                                    author: buffer.author,
                                    author_id: buffer.author_id,
                                    sent_at: buffer.sent_at,
                                    text: buffer.text,
                                    attachments: buffer.attachments,
                                };
                                let _ = tx.send(incoming).await;
                                return;
                            }
                        });
                    }
                    continue;
                }

                let incoming = IncomingMessage {
                    chat_id,
                    message_id: message.id.0.to_string(),
                    author,
                    author_id,
                    sent_at: message.date,
                    text: content.to_string(),
                    attachments,
                };

                // The receiver only goes away when `MessagingPlugin::on_disabled`
                // aborts this task anyway, but a failed send means it's already gone —
                // no point continuing to poll.
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
        let parsed_message_id: i32 = message_id
            .parse()
            .map_err(|_| PluginError::FailedUnknown(format!("invalid telegram message id: {message_id}")))?;
        self.send_chunks(chat_id, text, Some(MessageId(parsed_message_id))).await
    }

    async fn react_on_message(&self, chat_id: &str, message_id: &str, reaction: &str) -> Result<(), PluginError> {
        let parsed_chat_id: i64 = chat_id
            .parse()
            .map_err(|_| PluginError::FailedUnknown(format!("invalid telegram chat id: {chat_id}")))?;
        let parsed_message_id: i32 = message_id
            .parse()
            .map_err(|_| PluginError::FailedUnknown(format!("invalid telegram message id: {message_id}")))?;

        let mut request = self.bot.set_message_reaction(ChatId(parsed_chat_id), MessageId(parsed_message_id));
        request.reaction = Some(vec![ReactionType::Emoji { emoji: reaction.to_string() }]);
        request
            .await
            .map_err(|e| PluginError::FailedUnknown(format!("telegram set_message_reaction failed: {e}")))?;

        Ok(())
    }
}

impl TelegramProvider {
    /// Shared by `send_message` and `reply_message` — everything about getting `text`
    /// onto the chat except whether it's addressed to a prior message. `reply_to` (when
    /// given) is only attached to the *first* chunk: threading every split chunk back
    /// to the original message would just repeat the same quote block above each one,
    /// and the first chunk already anchors the whole reply for anyone reading the
    /// thread.
    async fn send_chunks(&self, chat_id: &str, text: &str, reply_to: Option<MessageId>) -> Result<(), PluginError> {
        let parsed_chat_id: i64 = chat_id
            .parse()
            .map_err(|_| PluginError::FailedUnknown(format!("invalid telegram chat id: {chat_id}")))?;

        // Telegram has no length limit the frontend UI ever had to deal with — a
        // reasoning-heavy reply that's fine to render as one page in a browser can
        // easily exceed `sendMessage`'s hard cap, which Telegram just rejects outright
        // rather than truncating. Split *after* converting (see `split_for_telegram`'s
        // own doc comment for why) and send each chunk as its own message.
        for (index, chunk) in split_for_telegram(&to_telegram_markdown_v2(text)).into_iter().enumerate() {
            let reply_parameters =
                (index == 0).then(|| reply_to.map(|message_id| ReplyParameters { message_id, ..Default::default() })).flatten();

            let mut formatted_request = self.bot.send_message(ChatId(parsed_chat_id), chunk.clone());
            formatted_request.parse_mode = Some(ParseMode::MarkdownV2);
            formatted_request.reply_parameters = reply_parameters.clone();

            // Best-effort: if MarkdownV2 sending fails (an edge case `to_telegram_markdown_v2`
            // didn't handle right, tripping Telegram's strict entity parser), fall back to
            // sending this chunk plain rather than losing it entirely — the reply itself
            // matters more than its formatting. The chunk is already MarkdownV2-escaped
            // text, so this fallback can show stray backslashes in the rare case it's
            // actually needed; a real, slightly-ugly reply beats a chunk that never sends.
            if let Err(err) = formatted_request.await {
                tracing::warn!("telegram send_message with MarkdownV2 failed ({err}), retrying as plain text");

                let mut plain_request = self.bot.send_message(ChatId(parsed_chat_id), chunk);
                plain_request.reply_parameters = reply_parameters;
                plain_request
                    .await
                    .map_err(|e| PluginError::FailedUnknown(format!("telegram send_message failed: {e}")))?;
            }
        }

        Ok(())
    }
}

impl TelegramProvider {
    /// Downloads the highest-resolution size of a photo (`sizes` is Telegram's own
    /// smallest-to-largest ordering, but picked by actual pixel count here rather than
    /// trusting that order) and returns it base64-encoded, no data-URL prefix — the
    /// wire format `Agent::chat`'s `images` wants. Two calls, same as any Bot API file
    /// download: `get_file` resolves the size's `file_id` into a real path, then
    /// `download_file` fetches the bytes from that path.
    async fn download_largest_photo(&self, sizes: &[PhotoSize]) -> Result<String, PluginError> {
        let largest = sizes
            .iter()
            .max_by_key(|size| size.width * size.height)
            .ok_or_else(|| PluginError::FailedUnknown("telegram photo message reported no sizes".to_string()))?;

        let file = self
            .bot
            .get_file(largest.file.id.clone())
            .await
            .map_err(|e| PluginError::FailedUnknown(format!("telegram get_file failed: {e}")))?;

        let mut bytes: Vec<u8> = Vec::new();
        self.bot
            .download_file(&file.path, &mut bytes)
            .await
            .map_err(|e| PluginError::FailedUnknown(format!("telegram download_file failed: {e}")))?;

        Ok(BASE64.encode(bytes))
    }
}

impl TelegramProvider {
    /// Fetches only the single most recent pending update (if any), via Telegram's own
    /// documented trick for this (`offset: -1` — "retrieve updates starting from
    /// -offset update from the end of the queue"; `timeout: 0` so it returns
    /// immediately instead of long-polling), and returns the offset that confirms
    /// everything up to and including it. Passing that as the next call's `offset`
    /// tells Telegram every earlier update has already been seen, so only genuinely
    /// new messages arrive from here on — see `TelegramProvider::offset`'s own doc
    /// comment for when this runs.
    async fn latest_offset(bot: &Bot) -> Result<i32, PluginError> {
        let mut request = bot.get_updates();
        request.offset = Some(-1);
        request.timeout = Some(0);

        let updates = request
            .await
            .map_err(|e| PluginError::FailedUnknown(format!("telegram get_updates (backlog check) failed: {e}")))?;

        Ok(updates.last().map(|update| update.id.as_offset()).unwrap_or(0))
    }
}
