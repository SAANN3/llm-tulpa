use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::mpsc::Sender;

use crate::plugins::base::PluginError;
use crate::tools::base::PropertyInfo;

/// One attachment on an incoming message — images only for now. A future kind (a
/// plain file, say) gets its own variant here rather than a new field on
/// `IncomingMessage`, so everything that reads attachments (`IncomingMessage::images`
/// today, a future `::files` alongside it) stays one filter over a single list per
/// provider, and adding that variant is a compiler-enforced exhaustiveness check at
/// every existing `match` on this type rather than something that can be forgotten.
#[derive(Debug, Clone)]
pub enum Attachment {
    /// Base64-encoded image data (no data-URL prefix) — same convention as
    /// `Agent::chat`'s own `images` parameter, which this is ultimately headed for.
    Image(String),
}

/// One message received from a messaging platform, in whatever minimal shape every
/// provider can produce regardless of the platform's own message model.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// The chat/channel/conversation this arrived on — platform-specific, opaque to
    /// everything above the provider. `MessagingPlugin` only ever compares it against
    /// `allowed_chat_ids` as a string, never interprets it.
    pub chat_id: String,
    /// This message's own platform-specific id — opaque, like `chat_id` — passed back
    /// into `reply_message`/`react_on_message` so a provider can act on the message
    /// that triggered it rather than just the chat it arrived on.
    pub message_id: String,
    pub author: String,
    /// This message's sender's platform-specific user id — opaque, like `chat_id`/
    /// `message_id`. Distinct from `author` (a display name, which two different users
    /// can share, and which a user can change): exists so `MessagingSettings::name_hint`
    /// (see `plugin.rs`) can tell senders apart reliably in a group chat.
    pub author_id: String,
    /// When the platform says this was actually sent — not when it reached this
    /// process, which can lag behind by however long a debounce/queue/reconnect added
    /// (see `MediaGroupBuffer` in `telegram.rs`, e.g.). Every provider's own message
    /// model carries this already, so there's no reason to substitute "now".
    pub sent_at: chrono::DateTime<chrono::Utc>,
    pub text: String,
    pub attachments: Vec<Attachment>,
}

impl IncomingMessage {
    /// Just the image attachments' base64 data, in order — what `Agent::chat` wants.
    /// A message can be text-only (empty), image-only (empty `text`), or both.
    pub fn images(&self) -> Vec<String> {
        self.attachments
            .iter()
            .map(|attachment| match attachment {
                Attachment::Image(data) => data.clone(),
            })
            .collect()
    }
}

/// The per-platform half of a messaging plugin (Discord, VK, Telegram, …) — everything
/// a concrete provider implements to plug into the generic `MessagingPlugin<Self>`.
/// Never implements `Plugin`/`PluginBuilder` directly — `MessagingPlugin`/
/// `MessagingProviderBuilder` supply those once, generically, so a new provider is
/// exactly this trait and nothing else (see `plugins/base.rs`'s doc comment on `Plugin`
/// for the same reasoning one level up).
#[async_trait]
pub trait MessagingProvider: Send + Sync + 'static {
    /// This provider's own settings — bot token, etc. Opaque to `MessagingPlugin`,
    /// which only ever stores and round-trips it, never reads a field of it (the one
    /// field `MessagingPlugin` does care about, `allowed_chat_ids`, lives one level up
    /// in `MessagingSettings` instead, precisely so it doesn't have to).
    type Settings: Send + Sync + Clone + Serialize + DeserializeOwned + 'static;

    /// This provider's `plugin_subname` (e.g. "discord", "vk") — combined with
    /// `MessagingPlugin::plugin_name` ("messaging") to key it in the registry.
    fn subname() -> &'static str;

    /// A step-by-step "how to use" message for this provider — e.g. Telegram's walks
    /// through finding the bot, starting a chat, and sending `/chat`. Shown as-is on
    /// the frontend's plugin settings panel (see `MessagingProviderBuilder::help_message`).
    fn help_message() -> String;

    /// This provider's own settings fields, for the frontend settings form —
    /// `MessagingProviderBuilder::settings_schema` appends this after the shared
    /// `allowed_chat_ids` field, giving one flat property list overall. In practice
    /// this is almost always just `Self::Settings::tool_properties()` — derive
    /// `ToolParams` on the concrete `Settings` struct (same `#[derive(ToolParams)]` +
    /// `#[tool(description = "...")]` pattern already used for tool arguments) and
    /// delegate to it here. Can't be done generically once, in `MessagingPlugin`
    /// itself: the derive macro reads each field's own concrete type to pick a
    /// `PropertyType`, so it needs `Self::Settings` to be a real, non-generic struct.
    fn settings_schema() -> Vec<PropertyInfo>;

    /// Builds a connected instance from its settings. Doesn't run the connection loop
    /// itself — that's `run`, called separately once the instance exists.
    async fn connect(settings: Self::Settings) -> Result<Self, PluginError>
    where
        Self: Sized;

    /// Runs the provider's connection loop until cancelled, pushing one
    /// `IncomingMessage` through `tx` for every message received. Only returns on
    /// error or cancellation — `MessagingPlugin::on_enabled` spawns this as its own
    /// task and aborts it in `on_disabled`.
    async fn run(&self, tx: Sender<IncomingMessage>) -> Result<(), PluginError>;

    /// Sends a message on the given chat, out of nowhere (not attached to any
    /// particular prior message) — the outgoing half. A direct call, not routed through
    /// the channel: only the incoming direction needs one, since that's the side driven
    /// by an external event the provider discovers on its own connection, not something
    /// `MessagingPlugin` can just call synchronously.
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<(), PluginError>;

    /// Sends a reply addressed to a specific prior message (`message_id`, from an
    /// `IncomingMessage`) rather than just dropping a new message into the chat — on
    /// platforms that support it, this shows up threaded/quoting the original, which
    /// matters once a chat has multiple messages in flight (e.g. a fast-typing user
    /// sending several before the first reply lands) so the reply's target stays
    /// unambiguous. Default implementation falls back to `send_message` for any
    /// provider that has no such notion.
    async fn reply_message(&self, chat_id: &str, message_id: &str, text: &str) -> Result<(), PluginError> {
        let _ = message_id;
        self.send_message(chat_id, text).await
    }

    /// Puts an emoji reaction on the given message (`message_id`, from an
    /// `IncomingMessage`) — e.g. acknowledging receipt before a reply is ready. Default
    /// implementation is a no-op for any provider with no such notion, rather than an
    /// error: reacting is always optional decoration, never something a caller should
    /// have to treat as a hard failure if it's unsupported.
    async fn react_on_message(&self, chat_id: &str, message_id: &str, reaction: &str) -> Result<(), PluginError> {
        let _ = (chat_id, message_id, reaction);
        Ok(())
    }
}
