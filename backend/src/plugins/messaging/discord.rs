use std::sync::OnceLock;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use serenity::all::{
    Attachment as DiscordAttachment, ChannelId, Client, Context, CreateMessage, EventHandler, GatewayIntents, Http, Message,
    MessageId, MessageReference, ReactionType, Ready, UserId,
};
use tokio::sync::mpsc::Sender;
use tool_derive::ToolParams;

use super::chunking::{split_top_level_blocks, strip_code_fence};
use super::provider::{Attachment, IncomingMessage, MessagingProvider};
use crate::plugins::base::PluginError;
use crate::tools::base::{PropertyInfo, PropertyType, ToolParams};

#[derive(Debug, Clone, Deserialize, Serialize, ToolParams)]
pub struct DiscordSettings {
    #[tool(description = "The bot token from the Discord Developer Portal.")]
    pub token: String,
}

/// Discord rejects any single message over 2000 UTF-16 code units. This budget stays
/// conservatively under that in plain byte count, same reasoning as Telegram's own
/// `TELEGRAM_MAX_MESSAGE_CHARS`.
const DISCORD_MAX_MESSAGE_CHARS: usize = 1900;

/// Sent back — as a reply, so the sender's client shows exactly what it's responding
/// to — instead of forwarding anything to the agent when `/chat` (or any other
/// `/command`) arrives with nothing after it. Discord has no `ForceReply`-style forced
/// reply UI like Telegram's, but the user manually replying to this prompt still
/// arrives as a normal message with `referenced_message` set — indistinguishable from
/// any other message already routed here via `is_reply_to_bot` below.
const BARE_COMMAND_PROMPT: &str = "What would you like to ask?(Reply to this message)";

/// Splits a leading `/command` (or `/command arg`) syntax off the front of `text`, if
/// present — same shape and reasoning as Telegram's own `split_command`: `(true, "")`
/// for a bare command with nothing after it, `(true, "the rest")` for a command with a
/// real message attached, `(false, text)` unchanged for anything that isn't a command
/// (a plain @mention or reply-to-bot, neither of which start with `/`).
fn split_command(text: &str) -> (bool, &str) {
    if !text.starts_with('/') {
        return (false, text);
    }

    match text.split_once(char::is_whitespace) {
        Some((_command, rest)) => (true, rest.trim_start()),
        None => (true, ""),
    }
}

/// Whether an attachment is an image Ollama can actually look at — checked via its
/// reported media type rather than filename extension, since Discord always fills
/// `content_type` in for anything it recognizes.
fn is_image_attachment(attachment: &DiscordAttachment) -> bool {
    attachment.content_type.as_deref().is_some_and(|content_type| content_type.starts_with("image/"))
}

/// Downloads every image attachment on a message and base64-encodes each one (no
/// data-URL prefix) — the wire format `Agent::chat`'s `images` wants. A download
/// failure is logged and that one attachment is dropped rather than failing the whole
/// message — the rest of what was sent (text, other images) still deserves a reply.
async fn download_image_attachments(attachments: &[DiscordAttachment]) -> Vec<Attachment> {
    let mut images = Vec::new();

    for attachment in attachments.iter().filter(|a| is_image_attachment(a)) {
        match attachment.download().await {
            Ok(bytes) => images.push(Attachment::Image(BASE64.encode(bytes))),
            Err(err) => tracing::warn!("discord: failed to download attachment {}: {err}", attachment.url),
        }
    }

    images
}

/// Strips a single leading `<@bot_id>`/`<@!bot_id>` mention token — Discord's own raw
/// syntax for an @mention in message content — so the agent sees the actual message
/// rather than the literal mention syntax, same reasoning as `split_command`. Only the
/// leading occurrence: a mention typed elsewhere in the sentence is left as-is rather
/// than trying to strip every occurrence, which risks mangling a message that
/// legitimately talks about the bot mid-sentence.
fn strip_bot_mention(text: &str, bot_id: UserId) -> &str {
    let trimmed = text.trim_start();
    for prefix in [format!("<@{bot_id}>"), format!("<@!{bot_id}>")] {
        if let Some(rest) = trimmed.strip_prefix(prefix.as_str()) {
            return rest.trim_start();
        }
    }
    trimmed
}

/// Splits `text` into chunks under `DISCORD_MAX_MESSAGE_CHARS`, breaking only at the
/// top-level blank-line boundaries `split_top_level_blocks` finds — never inside a
/// fenced code block. Unlike Telegram, this runs on the *raw* agent reply with no
/// conversion step first: Discord's own markdown flavor already matches the model's
/// CommonMark-ish output closely enough (`**bold**`, `*italic*`, `` `code` ``, fenced
/// blocks, `~~strikethrough~~`, `#` headings, `-`/`1.` lists all render the same way),
/// and — unlike Telegram — Discord's message parser is lenient: stray punctuation like
/// an unescaped `(` never gets a message rejected, it just renders literally. So there's
/// no equivalent of `to_telegram_markdown_v2`'s escaping pass needed here at all.
fn split_for_discord(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for block in split_top_level_blocks(text) {
        for piece in split_block(block, DISCORD_MAX_MESSAGE_CHARS) {
            let separator_len = if current.is_empty() { 0 } else { 2 };
            if !current.is_empty() && current.len() + separator_len + piece.len() > DISCORD_MAX_MESSAGE_CHARS {
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
/// Telegram's `split_block` — a bare hard cut would leave a piece with a dangling
/// opener and no closer.
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
/// each wrapped in its own complete ` ```<lang> ` … ` ``` ` fence — mirrors Telegram's
/// `split_code_fence`, minus the escape-pair concern (Discord's `content` here is the
/// model's raw text, never escaped, so a plain `hard_split` cut is always safe).
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
/// escape-pair awareness needed (unlike Telegram's `hard_split`): nothing in `block` is
/// ever escaped, so any char boundary is a safe cut point.
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

/// The gateway-side event handler — separate from `DiscordProvider` itself since
/// `serenity::Client` takes ownership of it, while `DiscordProvider` has to keep
/// existing across `run()`'s own lifetime (and across `run()` calls, if the plugin is
/// disabled/re-enabled). `bot_id` is populated once, from the `ready` event, and read
/// on every message after that to tell the bot's own messages/mentions/replies-to-it
/// apart from anyone else's — `OnceLock` rather than a channel or `Mutex` since it's
/// written exactly once and read many times, never contended.
struct Handler {
    tx: Sender<IncomingMessage>,
    bot_id: OnceLock<UserId>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        let _ = self.bot_id.set(ready.user.id);
        tracing::info!("discord provider connected as {}", ready.user.name);
    }

    /// Discord — unlike Telegram's privacy-mode-filtered `getUpdates` — hands every
    /// message in a channel the bot can see straight to this handler, so the filtering
    /// Telegram gets for free has to happen here by hand: only a `/command`, an
    /// @mention of the bot, or a reply to one of the bot's own messages is treated as
    /// addressed to it. Everything else (ordinary conversation between other people in
    /// the same channel) is silently ignored.
    async fn message(&self, ctx: Context, new_message: Message) {
        if new_message.author.bot {
            return;
        }
        let Some(&bot_id) = self.bot_id.get() else {
            return;
        };

        let raw_text = new_message.content.trim();
        let (is_command, after_command) = split_command(raw_text);
        let has_images = new_message.attachments.iter().any(is_image_attachment);

        if is_command && after_command.is_empty() && !has_images {
            let prompt =
                CreateMessage::new().content(BARE_COMMAND_PROMPT).reference_message(MessageReference::from((
                    new_message.channel_id,
                    new_message.id,
                )));
            if let Err(err) = new_message.channel_id.send_message(&ctx.http, prompt).await {
                tracing::warn!("discord: failed to send bare-command prompt in channel {}: {err}", new_message.channel_id);
            }
            return;
        }

        let is_reply_to_bot = new_message.referenced_message.as_ref().is_some_and(|referenced| referenced.author.id == bot_id);
        let is_mentioned = new_message.mentions.iter().any(|user| user.id == bot_id);

        if !is_command && !is_reply_to_bot && !is_mentioned {
            return;
        }

        let content = if is_command { after_command } else { strip_bot_mention(raw_text, bot_id) };
        let content = content.trim();
        if content.is_empty() && !has_images {
            return;
        }

        let attachments = download_image_attachments(&new_message.attachments).await;

        let author = new_message.author.global_name.clone().unwrap_or_else(|| new_message.author.name.clone());
        let incoming = IncomingMessage {
            chat_id: new_message.channel_id.to_string(),
            message_id: new_message.id.to_string(),
            author,
            author_id: new_message.author.id.to_string(),
            sent_at: chrono::DateTime::from_timestamp(new_message.timestamp.unix_timestamp(), 0).unwrap_or_else(chrono::Utc::now),
            text: content.to_string(),
            attachments,
        };

        // Best-effort — the receiver only goes away when `MessagingPlugin::on_disabled`
        // aborts this whole task anyway, so a failed send here just means that's
        // already happened and this particular message is moot.
        let _ = self.tx.send(incoming).await;
    }
}

pub struct DiscordProvider {
    token: String,
    /// Used directly by `send_message`/`reply_message`/`react_on_message` — outbound
    /// REST calls need no gateway connection, so this is independent of whatever
    /// `Client` `run()` builds (and rebuilds, across enable/disable cycles).
    http: Http,
}

#[async_trait]
impl MessagingProvider for DiscordProvider {
    type Settings = DiscordSettings;

    fn subname() -> &'static str {
        "discord"
    }

    fn settings_schema() -> Vec<PropertyInfo> {
        DiscordSettings::tool_properties()
    }

    fn help_message() -> String {
        "How to use the Discord plugin:\n\
         \n\
         1. Go to the Discord Developer Portal (discord.com/developers/applications), create an application, then add a Bot to it and copy its token.\n\
         2. Under the bot's settings, turn on the \"Message Content Intent\" — without it the bot can't read message text.\n\
         3. Paste the bot token into this plugin's \"token\" setting and save.\n\
         4. Under OAuth2 → URL Generator, check the \"bot\" scope and the \"Send Messages\", \"Read Message History\", and \"Add Reactions\" permissions, then open the generated URL to invite the bot to your server.\n\
         5. Turn on Developer Mode in Discord (User Settings → Advanced), then right-click the channel you want the bot to use and \"Copy Channel ID\".\n\
         6. Add that channel id to \"allowed_chat_ids\" and save — the bot only ever responds in channels listed there.\n\
         7. Enable the plugin with the toggle.\n\
         8. In the channel, send /chat followed by your message (e.g. \"/chat what's the weather like\"), @mention the bot, or reply to one of its messages.\n\
         9. The bot reacts with 👀 to show it received your message, then replies threaded to it once the answer is ready."
            .to_string()
    }

    async fn connect(settings: Self::Settings) -> Result<Self, PluginError> {
        Ok(Self { http: Http::new(&settings.token), token: settings.token })
    }

    /// Builds a fresh gateway `Client` and runs it until it errors or the process
    /// disconnects — a new `Client` every call rather than one kept on `self`, since
    /// `Client::start` needs `&mut self` and `MessagingProvider::run` only gets `&self`
    /// (same instance is shared with the outbound trait methods, which need to keep
    /// working independent of the gateway connection's own lifetime). Cheap either way:
    /// unlike Telegram's `offset`, there's no cross-`run()`-call state to preserve —
    /// Discord's gateway doesn't have a "catch up on missed messages" concept, a fresh
    /// connection just starts receiving from whatever's live from here on.
    async fn run(&self, tx: Sender<IncomingMessage>) -> Result<(), PluginError> {
        let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::DIRECT_MESSAGES | GatewayIntents::MESSAGE_CONTENT;
        let handler = Handler { tx, bot_id: OnceLock::new() };

        let mut client = Client::builder(&self.token, intents)
            .event_handler(handler)
            .await
            .map_err(|e| PluginError::FailedUnknown(format!("failed to build discord client: {e}")))?;

        client.start().await.map_err(|e| PluginError::FailedUnknown(format!("discord client error: {e}")))
    }

    async fn send_message(&self, chat_id: &str, text: &str) -> Result<(), PluginError> {
        self.send_chunks(chat_id, text, None).await
    }

    async fn reply_message(&self, chat_id: &str, message_id: &str, text: &str) -> Result<(), PluginError> {
        let message_id = parse_message_id(message_id)?;
        self.send_chunks(chat_id, text, Some(message_id)).await
    }

    async fn react_on_message(&self, chat_id: &str, message_id: &str, reaction: &str) -> Result<(), PluginError> {
        let channel_id = parse_channel_id(chat_id)?;
        let message_id = parse_message_id(message_id)?;

        channel_id
            .create_reaction(&self.http, message_id, ReactionType::Unicode(reaction.to_string()))
            .await
            .map_err(|e| PluginError::FailedUnknown(format!("discord create_reaction failed: {e}")))?;

        Ok(())
    }
}

impl DiscordProvider {
    /// Shared by `send_message` and `reply_message` — everything about getting `text`
    /// onto the channel except whether it's addressed to a prior message. `reply_to`
    /// (when given) is only attached to the *first* chunk, same reasoning as
    /// Telegram's `send_chunks`: threading every split chunk back to the original
    /// message would just repeat the same quote block above each one.
    async fn send_chunks(&self, chat_id: &str, text: &str, reply_to: Option<MessageId>) -> Result<(), PluginError> {
        let channel_id = parse_channel_id(chat_id)?;

        for (index, chunk) in split_for_discord(text).into_iter().enumerate() {
            let mut builder = CreateMessage::new().content(chunk);
            if index == 0 && let Some(message_id) = reply_to {
                builder = builder.reference_message(MessageReference::from((channel_id, message_id)));
            }

            channel_id
                .send_message(&self.http, builder)
                .await
                .map_err(|e| PluginError::FailedUnknown(format!("discord send_message failed: {e}")))?;
        }

        Ok(())
    }
}

fn parse_channel_id(chat_id: &str) -> Result<ChannelId, PluginError> {
    chat_id.parse().map_err(|_| PluginError::FailedUnknown(format!("invalid discord channel id: {chat_id}")))
}

fn parse_message_id(message_id: &str) -> Result<MessageId, PluginError> {
    message_id.parse().map_err(|_| PluginError::FailedUnknown(format!("invalid discord message id: {message_id}")))
}
