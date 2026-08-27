use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tool_derive::ToolParams;

use super::provider::{IncomingMessage, MessagingProvider};
use crate::facade::agent::Agent;
use crate::plugins::base::{Plugin, PluginError};
use crate::services::chat_store::ChatStore;
use crate::tools::base::{PropertyInfo, PropertyType, ToolParams};

/// How many messages can queue up between the provider's connection loop and the read
/// loop below before the provider's `tx.send` starts blocking — generous enough that a
/// burst of messages doesn't stall the provider's own loop (e.g. delay it acking or
/// polling), small enough that a genuinely stuck consumer still applies backpressure
/// instead of buffering unboundedly.
const CHANNEL_CAPACITY: usize = 64;

/// Reaction put on a message as soon as it's accepted for processing (see
/// `on_new_message`) — an immediate "seen, working on it" acknowledgement for the
/// stretch of time before a reply is ready, which on a thinking-heavy reply can be
/// long enough that the user might otherwise wonder if the bot saw the message at all.
const RECEIVED_REACTION: &str = "👀";

/// How many times `run_with_reconnect` (below) retries a provider's `run()` loop after
/// it ends in error before giving up on the connection entirely — a flat cap per
/// `on_enabled` lifecycle, not reset on a later long-lived success, since `run()` gives
/// no way to tell "reconnected and healthy" apart from "just started" without adding
/// provider-level signaling nothing needs yet.
const MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Wait before each reconnect attempt, longest last — index 0 is the wait before the
/// 1st retry, and so on. Index count must match `MAX_RECONNECT_ATTEMPTS`.
const RECONNECT_BACKOFF: [Duration; MAX_RECONNECT_ATTEMPTS as usize] =
    [Duration::from_secs(2), Duration::from_secs(5), Duration::from_secs(15)];

/// Runs `provider.run(tx)` and, if it ends in error, retries with increasing backoff up
/// to `MAX_RECONNECT_ATTEMPTS` times before giving up for good — a transient network
/// blip (the kind long-polling APIs like Telegram's hit routinely) would otherwise
/// permanently kill the connection until someone manually disables and re-enables the
/// plugin. `run()` returning `Ok(())` means an intentional stop (the read loop's
/// receiver was dropped, i.e. the channel closed), not a failure — that ends this loop
/// too, without retrying.
async fn run_with_reconnect<P: MessagingProvider>(provider: Arc<P>, tx: mpsc::Sender<IncomingMessage>, plugin_label: &str) {
    let mut attempt = 0;

    loop {
        match provider.run(tx.clone()).await {
            Ok(()) => {
                tracing::info!("messaging provider loop for {plugin_label} stopped");
                return;
            }
            Err(err) => {
                if attempt >= MAX_RECONNECT_ATTEMPTS {
                    tracing::error!(
                        "messaging provider loop for {plugin_label} failed after {MAX_RECONNECT_ATTEMPTS} reconnect attempts, giving up: {err}"
                    );
                    return;
                }

                let wait = RECONNECT_BACKOFF[attempt as usize];
                attempt += 1;
                tracing::warn!(
                    "messaging provider loop for {plugin_label} ended (reconnect attempt {attempt}/{MAX_RECONNECT_ATTEMPTS} in {wait:?}): {err}"
                );
                tokio::time::sleep(wait).await;
            }
        }
    }
}

/// Settings shared by every messaging provider, wrapping the provider's own.
/// `allowed_chat_ids` lives here — once, generically — rather than being redefined per
/// provider, since filtering is `MessagingPlugin`'s job, not the provider's: a provider
/// only ever reports what arrived, and whether to act on it is decided here, before
/// anything downstream (like the agent) is ever invoked. Empty means nothing is
/// allowed yet, not "everything" — the same fail-closed default `PluginRegistry`
/// enforces for `enabled` applies here too.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagingSettings<S> {
    pub allowed_chat_ids: Vec<String>,
    /// Forwarded as-is to `Agent::chat`'s own `think` parameter for every reply this
    /// provider sends — a per-provider setting rather than a global app one, since how
    /// much a few extra seconds of latency matters is genuinely different in a chat app
    /// (someone waiting on Telegram) than in llm-tulpa's own UI (already watching a
    /// spinner either way). `#[serde(default)]` so settings saved before this field
    /// existed still deserialize (as `false`) instead of failing to rebuild at the next
    /// startup or settings update.
    #[serde(default)]
    pub think: bool,
    /// `#[serde(flatten)]` — `settings_schema` (below) reports `allowed_chat_ids` and
    /// every field `P::Settings` has as one flat list (`PropertyInfo` has no concept of
    /// nesting; it's the same flat leaf-list shape used for LLM tool-calling args), so
    /// the actual JSON has to be flat too, or a settings form built from that schema
    /// would submit a shape `build()` can't deserialize. Without this, `provider`
    /// would serialize as its own nested `{"provider": {...}}` sub-object instead.
    #[serde(flatten)]
    pub provider: S,
}

/// Exists only so `#[derive(ToolParams)]` has a concrete, non-generic struct to read —
/// `MessagingSettings<S>` can't derive it directly, since the macro inspects each
/// field's own type to pick a `PropertyType`, and `provider: S` is generic. Field name
/// and type are kept in sync with `MessagingSettings::allowed_chat_ids` by hand; the
/// two aren't the same struct, but this one is never constructed, only ever asked for
/// its `tool_properties()`.
#[derive(ToolParams)]
struct MessagingSharedSettingsSchema {
    #[tool(description = "Chat/channel IDs this provider is allowed to act on — anything else is silently ignored.")]
    #[allow(dead_code)]
    allowed_chat_ids: Vec<String>,
    #[tool(description = "Whether replies think before answering — slower, but can give a more careful answer.")]
    #[allow(dead_code)]
    think: bool,
}

/// The shared `allowed_chat_ids` field's schema, followed by whatever `P` itself
/// contributes — used by `MessagingProviderBuilder::settings_schema` to build one flat
/// property list for the frontend settings form.
pub fn settings_schema<P: MessagingProvider>() -> Vec<PropertyInfo> {
    let mut properties = MessagingSharedSettingsSchema::tool_properties();
    properties.extend(P::settings_schema());
    properties
}

/// The two background tasks a running instance owns — the provider's own connection
/// loop, and the loop reading what it sends. Kept together so `on_disabled` can't
/// abort one and forget the other.
struct RunningLoops {
    run: JoinHandle<()>,
    read: JoinHandle<()>,
}

/// The generic half of a messaging plugin — implements `Plugin` once for every
/// provider `P`, so a concrete provider (Discord, VK, …) only ever implements
/// `MessagingProvider`, never this trait directly. See `plugins/base.rs`'s doc comment
/// on `Plugin` for why.
pub struct MessagingPlugin<P: MessagingProvider> {
    provider: Arc<P>,
    settings: MessagingSettings<P::Settings>,
    /// `None` until `on_enabled` runs. `tokio::sync::Mutex`, matching the rest of
    /// `plugins/`'s locks — only ever held briefly to swap the whole value, never
    /// across other work.
    running: Mutex<Option<RunningLoops>>,
    /// Own `Agent`, built (in `main.rs`) with an empty `ToolService` — a plugin chat
    /// gets real persistence and a real reply, with zero tool-calling risk, sharing
    /// `ollama`/`chat_store`/`permission_store` with the main app's `Agent` rather than
    /// duplicating those services.
    agent: Arc<Agent>,
    /// Used to resolve an incoming message's platform-specific `chat_id` into the
    /// internal `Chat` `agent` actually talks to — see `on_new_message`.
    chat_store: Arc<ChatStore>,
}

impl<P: MessagingProvider> MessagingPlugin<P> {
    pub fn new(provider: P, settings: MessagingSettings<P::Settings>, agent: Arc<Agent>, chat_store: Arc<ChatStore>) -> Self {
        Self { provider: Arc::new(provider), settings, running: Mutex::new(None), agent, chat_store }
    }

    /// One incoming message's worth of work — filters by `allowed_chat_ids`, resolves
    /// (or creates) the internal chat mapped to `message.chat_id`, gets the agent's
    /// reply, and sends it back via `provider`. A plain function, not a `&self` method:
    /// it runs inside the read loop spawned by `on_enabled`, which can't borrow `self`
    /// (a `Plugin` method only ever gets `&self`, never an owned `Arc<Self>` it could
    /// clone into a `'static` task — see `on_enabled` below) — so everything it needs
    /// is passed in directly instead, already cloned out before the loop was spawned.
    /// Every failure is logged and dropped rather than propagated — this runs
    /// unattended inside a background loop with nothing to report a `Result` to.
    async fn on_new_message(
        message: IncomingMessage,
        allowed_chat_ids: &[String],
        think: bool,
        provider: &Arc<P>,
        agent: &Arc<Agent>,
        chat_store: &Arc<ChatStore>,
    ) {
        tracing::info!(
            "messaging plugin: received message from {} (chat_id {})",
            message.author,
            message.chat_id
        );

        if !allowed_chat_ids.iter().any(|id| id == &message.chat_id) {
            // Logged at `info`, not silently dropped — this is the only way to learn a
            // chat's id in the first place (Telegram doesn't expose it up front), so
            // the very first message from a not-yet-allowed chat needs to be visible
            // here for that id to ever make it into `allowed_chat_ids`.
            tracing::info!("messaging plugin: ignoring chat_id {} — not in allowed_chat_ids", message.chat_id);
            return;
        }

        // Best-effort, not fatal if it fails — a missing reaction is a minor cosmetic
        // gap, not a reason to skip actually answering the message.
        if let Err(err) = provider.react_on_message(&message.chat_id, &message.message_id, RECEIVED_REACTION).await {
            tracing::warn!("messaging plugin: failed to react to message in chat {}: {err}", message.chat_id);
        }

        let chat = match chat_store
            .find_or_create_plugin_chat(
                message.author.clone(),
                "messaging".to_string(),
                P::subname().to_string(),
                message.chat_id.clone(),
            )
            .await
        {
            Ok(chat) => chat,
            Err(err) => {
                let err: crate::services::error::ErrorService = err.into();
                tracing::warn!(
                    "messaging plugin: couldn't resolve chat for {}: {}",
                    message.chat_id,
                    err.message.as_deref().unwrap_or("unknown error")
                );
                return;
            }
        };

        let reply = match agent.chat(chat.id, message.text, Some(think)).await {
            Ok(reply) => reply,
            Err(err) => {
                tracing::warn!(
                    "messaging plugin: agent call failed for chat {}: {}",
                    chat.id,
                    err.message.as_deref().unwrap_or("unknown error")
                );
                return;
            }
        };

        match provider.reply_message(&message.chat_id, &message.message_id, &reply.content).await {
            Ok(()) => tracing::info!("messaging plugin: replied to chat_id {}", message.chat_id),
            Err(err) => tracing::warn!("messaging plugin: failed to send reply to {}: {err}", message.chat_id),
        }
    }
}

#[async_trait]
impl<P: MessagingProvider> Plugin for MessagingPlugin<P> {
    fn plugin_name(&self) -> &str {
        "messaging"
    }

    fn plugin_subname(&self) -> &str {
        P::subname()
    }

    fn settings_value(&self) -> Value {
        serde_json::to_value(&self.settings).unwrap_or(Value::Null)
    }

    fn api_router(&self) -> Router {
        // No endpoints yet — a messaging provider runs entirely off its own
        // connection loop, nothing to serve over HTTP until there's a real reason to
        // (e.g. a webhook-based provider, or a settings sub-resource).
        Router::new()
    }

    async fn on_enabled(&self) -> Result<(), PluginError> {
        let plugin_label = format!("{}/{}", self.plugin_name(), P::subname());
        let (tx, mut rx) = mpsc::channel::<IncomingMessage>(CHANNEL_CAPACITY);

        let run_provider = self.provider.clone();
        let run_label = plugin_label.clone();
        let run = tokio::spawn(async move {
            run_with_reconnect(run_provider, tx, &run_label).await;
        });

        let allowed_chat_ids = self.settings.allowed_chat_ids.clone();
        let think = self.settings.think;
        let read_provider = self.provider.clone();
        let agent = self.agent.clone();
        let chat_store = self.chat_store.clone();
        let read = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                Self::on_new_message(message, &allowed_chat_ids, think, &read_provider, &agent, &chat_store).await;
            }
        });

        *self.running.lock().await = Some(RunningLoops { run, read });
        tracing::info!("messaging plugin {plugin_label} enabled");
        Ok(())
    }

    async fn on_disabled(&self) -> Result<(), PluginError> {
        // Intentional shutdown (the plugin is being disabled), not the unintended
        // cascade-cancellation case the join-not-abort lesson elsewhere in this
        // project is about — `.abort()` is the right call here.
        if let Some(loops) = self.running.lock().await.take() {
            loops.run.abort();
            loops.read.abort();
            tracing::info!("messaging plugin {}/{} disabled", self.plugin_name(), P::subname());
        }
        Ok(())
    }
}
