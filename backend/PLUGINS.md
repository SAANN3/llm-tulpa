# Plugins
Optional integrations that live outside the core chat app — chat platforms today (Telegram, Discord, VK), each configured and switched on/off through its own settings.

## How it works
Every plugin type implements two traits ([`src/plugins/base.rs`](./src/plugins/base.rs)):

```rust
trait Plugin: Send + Sync {
    fn plugin_name(&self) -> &str;
    fn plugin_subname(&self) -> &str;
    fn settings_value(&self) -> Value;
    fn api_router(&self) -> Router;
    async fn on_enabled(&self) -> Result<(), PluginError>;
    async fn on_disabled(&self) -> Result<(), PluginError>;
}

trait PluginBuilder: Send + Sync {
    fn plugin_name(&self) -> &str;
    fn plugin_subname(&self) -> &str;
    fn settings_schema(&self) -> Vec<PropertyInfo>;
    fn help_message(&self) -> String;
    async fn build(&self, settings: Value) -> Result<Arc<dyn Plugin>, PluginError>;
}
```

`plugin_name` groups instances that share an API shape and settings contract (e.g. `"messaging"`); `plugin_subname` picks the concrete implementation (e.g. `"telegram"`). `PluginBuilder` is the stateless factory — a settings change is handled by discarding the old `Plugin` instance and building a fresh one via `build`, not an in-place update, so a plugin never has to reason about a partially-applied settings change.

`settings_schema`/`help_message` reuse the same `PropertyInfo` schema tool-calling args use (see [TOOLS.md](./TOOLS.md)) — one generic frontend form renderer for both, instead of hand-built UI per plugin.

`PluginRegistry` ([`src/plugins/registry.rs`](./src/plugins/registry.rs)) holds every registered plugin keyed by `(plugin_name, plugin_subname)`, persists settings/enabled state to Postgres, and mounts each plugin's own `api_router()` under a stable proxy path that looks up the live instance fresh per request — so a settings change or enable/disable never requires touching axum's route tree.

## Messaging plugin type
`messaging` is the one plugin type that exists today — chat platforms as interchangeable subplugins. A concrete provider only ever implements `MessagingProvider` ([`src/plugins/messaging/provider.rs`](./src/plugins/messaging/provider.rs)):

```rust
trait MessagingProvider: Send + Sync + 'static {
    type Settings: Send + Sync + Clone + Serialize + DeserializeOwned + 'static;

    fn subname() -> &'static str;
    fn help_message() -> String;
    fn settings_schema() -> Vec<PropertyInfo>;
    async fn connect(settings: Self::Settings) -> Result<Self, PluginError>;
    async fn run(&self, tx: Sender<IncomingMessage>) -> Result<(), PluginError>;
    async fn send_message(&self, chat_id: &str, text: &str) -> Result<(), PluginError>;
    async fn reply_message(&self, chat_id: &str, message_id: &str, text: &str) -> Result<(), PluginError> { .. } // defaults to send_message
    async fn react_on_message(&self, chat_id: &str, message_id: &str, reaction: &str) -> Result<(), PluginError> { .. } // defaults to a no-op
}
```

A provider never implements `Plugin`/`PluginBuilder` directly — a generic `MessagingPlugin<P>`/`MessagingProviderBuilder<P>` ([`plugin.rs`](./src/plugins/messaging/plugin.rs), [`builder.rs`](./src/plugins/messaging/builder.rs)) implements those once for every provider.

On enable, `MessagingPlugin` spawns the provider's own `run()` loop (long-poll, gateway connection, whatever that platform needs) alongside a read loop that, for every incoming message: checks it's from an `allowed_chat_ids` entry (anything else is logged and ignored, not answered), reacts to acknowledge receipt (best-effort), resolves or creates the internal chat that platform conversation maps to, gets a reply from its own `Agent` (no tool-calling — a plugin conversation gets real persistence and a real reply with zero tool-calling risk), and sends it back threaded via `reply_message`.

Every provider shares `allowed_chat_ids`, `think` (whether a reply thinks before answering), `name_hint`, and `timezone` on top of its own settings. `name_hint`, off by default, tags every message handed to the agent with who actually sent it — `[Message from user named X with userid X, sent at ...]` — so a bot that's talking to several people (a group chat, most obviously) doesn't see them as one ongoing conversation; `timezone` (an IANA name, defaulting to UTC) is what the send time in that tag is shown in. Any images attached to an incoming message (a photo, or several sent as one album/attachment group) are forwarded to the agent the same way the frontend's own image attachments are — every image is decoded and re-encoded before it reaches Ollama, so a format Ollama's own decoder can't handle gets caught here instead of failing the whole reply. Splitting a long reply into several platform messages ([`plugins/messaging/chunking.rs`](./src/plugins/messaging/chunking.rs)) is shared too — it only tracks blank lines and fenced code blocks, nothing platform-specific, so every provider reuses it and only implements its own last-resort hard-cut on top.

## Adding a provider
1. Add a `Settings` struct (`#[derive(ToolParams)]`, same pattern as a tool's args struct — see [TOOLS.md](./TOOLS.md)) and implement `MessagingProvider` for it.
2. Register it in [`main.rs`](./src/main.rs): add `Arc::new(MessagingProviderBuilder::<YourProvider>::new(plugin_agent.clone(), chat_store.clone()))` to the `plugin_builders` list.

That's it — HTTP routes, settings persistence, and the frontend settings form are all generic over `MessagingProvider`.

## Current plugins

### `messaging` — chat platforms
| Subname | Settings | Notes |
|---|---|---|
| `telegram` | `token` | Long-polls the Bot API. |
| `discord` | `token` | Persistent gateway connection; only responds to a `/chat` command, an @mention, or a reply to one of its own messages. |
| `vk` | `token`, `group_id` | Long-polls VK's Bots Long Poll API. The community token needs both the "Messages" and "Manage community" access rights, and the community itself needs the Bots Long Poll API — and its "Message received" event type specifically — turned on. |

Each provider's own step-by-step setup instructions are available from its settings panel in the app (or `GET /api/plugins/help`), not duplicated here.

## HTTP API
Registry-level management routes, under `/api/plugins`:

| Route | What it does |
|---|---|
| `GET /` | Every registered plugin, enabled and disabled alike, with its current settings. |
| `GET /settings_schema?plugin_name=&plugin_subname=` | A plugin's settings schema, same shape as a tool's args. |
| `GET /help?plugin_name=&plugin_subname=` | A plugin's own step-by-step "how to use" message. |
| `POST /settings` | Sets (or replaces) a plugin's settings, rebuilding its live instance. |
| `POST /enable` | Enables or disables a plugin — fails if it has no settings configured yet. |
| `POST /reset_chat` | Wipes a plugin-linked chat's message history, keeping the chat and its external mapping intact — useful for retesting behavior without a stale reply from earlier still in the history the model gets replayed on every turn. |

A plugin's own instance-level routes (`Plugin::api_router()`) mount under `/api/plugins/{plugin_name}/{plugin_subname}/...` — none of the current providers expose any yet.
