# Plugins
Optional integrations that live outside the core chat app — chat platforms (Telegram, Discord, VK) each configured and switched on/off through its own settings, and standalone tool-contributing plugins like `coding` that need no settings at all.

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
    fn tools(&self) -> Vec<Box<dyn Tool>> { vec![] } // defaults to none
    async fn prompt_section(&self, user_id: i64, tools: bool) -> Option<String> { None } // defaults to nothing
}

trait PluginBuilder: Send + Sync {
    fn plugin_name(&self) -> &str;
    fn plugin_subname(&self) -> &str;
    fn settings_schema(&self) -> Vec<PropertyInfo>;
    fn summary(&self) -> &str;
    fn help_message(&self) -> String;
    async fn build(&self, settings: Value) -> Result<Arc<dyn Plugin>, PluginError>;
}
```

`plugin_name` groups instances that share an API shape and settings contract (e.g. `"messaging"`); `plugin_subname` picks the concrete implementation (e.g. `"telegram"`). `PluginBuilder` is the stateless factory — a settings change is handled by discarding the old `Plugin` instance and building a fresh one via `build`, not an in-place update, so a plugin never has to reason about a partially-applied settings change.

`summary` is one short line on what the plugin does, shown under its name in the plugin list (which comes sorted by `plugin_name`, then `plugin_subname`). `settings_schema`/`help_message` reuse the same `PropertyInfo` schema tool-calling args use (see [TOOLS.md](./TOOLS.md)) — one generic frontend form renderer for both, instead of hand-built UI per plugin.

`PluginRegistry` ([`src/plugins/registry.rs`](./src/plugins/registry.rs)) holds every registered plugin keyed by `(plugin_name, plugin_subname)`, persists settings/enabled state to Postgres, and serves each plugin's own `api_router()` through one catch-all proxy route (`routes/plugins/proxy.rs`) that looks up the live instance in the registry on every request — so a settings change or enable/disable never requires touching axum's route tree, and the routes exist even when the backend started before a database was configured. Plugins run under the **owner's** account (their settings are the owner's, their chats belong to the owner), so every `/api/plugins/*` route — management and proxied alike — is owner-only. A plugin whose stored settings its builder now rejects is logged and left unconfigured instead of failing startup. A plugin whose `settings_schema()` is empty needs no settings input at all — `register` gives it a built `{}` instance right away instead of leaving it stuck unconfigured forever.

A plugin's `tools()` (default: none) are registered in the shared `ToolService` — the same set the main agent calls from — the moment it's enabled, and unregistered the moment it's disabled, live, with no restart. `ToolService` validates a plugin's whole tool batch against every already-registered name (built-in and other plugins' alike) before adding any of them, so an enable either adds all of a plugin's tools or none of them; a collision surfaces as `409` from `POST /api/plugins/enable` or `POST /api/plugins/settings` (the latter when a settings change would swap in a new instance whose tools collide) and leaves the plugin disabled.

A plugin's `prompt_section(user_id, tools)` (default: nothing; async, and per user, since what a plugin has for one user, their own skills say, is not what it has for the next) is a text it adds to the system prompt while it is enabled, for what the model should check before using the plugin's tools. `tools` says whether the chat sends the model its tools, so a section about tools can leave itself out of a chat that has none. `PluginRegistry::prompt_sections` joins the sections of the enabled plugins in the order of their keys, each as long as the plugin wrote it (enabling a plugin is the user's own choice, so nothing is cut), and the main agent puts them after the built-in or custom prompt and before the sub-agent note and the summary; Chat info → Context shows them as their own part, "Plugin prompts". The text has to be the same every time it is asked: the prompt is matched against the model server's cache, so a section that changes (a date, a counter) makes it read the whole chat again every turn. Enabling or disabling a plugin changes the prompt once. Two plugins registered under one `(plugin_name, plugin_subname)` are refused at startup, since they would replace each other. The messaging plugins' agent has no tools and takes no sections.

A plugin that needs tables of its own declares them with `PluginBuilder::migrations(prefix)`: ordered SQL steps, written with the prefix the registry hands over (`plugin_<name>_<subname>`, so `format!("CREATE TABLE {prefix}_items (...)")`). `PluginRegistry::register` applies the steps the database has not seen yet before the plugin is registered, enabled or not, in order, each in a transaction of its own with the plugin's entry in the `plugin_migrations` ledger ([`plugins/schema.rs`](./src/plugins/schema.rs)). Steps only add: `CREATE TABLE`, `CREATE INDEX`, `ALTER TABLE`, `INSERT` and `UPDATE`, every table and index named with the prefix, no drops, truncates or deletes and no quoted names; a step that reaches outside the prefix (a core table such as `users`) is refused before anything runs, and a foreign key to a core table (`REFERENCES users (id) ON DELETE CASCADE`) is fine. To change a table, add a step; never edit or reorder one that has been released, since the database only counts how many it has applied. A plugin whose steps are refused or fail is logged and left out, and the backend and the other plugins carry on; it is retried at the next start. A database that has more of a plugin's steps applied than the build knows is refused, like a newer schema. A plugin's own queries use the same prefix.

A plugin can have a tab of its own in Settings, declared as data ([`plugins/tab.rs`](./src/plugins/tab.rs)): `Plugin::settings_tab()` gives its id (its place in the address, `/settings/<id>`) and title, `tab_layout(user_id)` what it shows that user, `tab_action(user_id, action, args)` what a control sends back and `tab_upload(user_id, action, files)` what was dropped on a drop zone; each answers with the layout as it is after. A layout is sections of blocks: text, a switch (`{"on": bool}`), a list of rows each with a title, subtitle, badge, switch (`{"row", "on"}`) and buttons (`{"row"}`, optionally asked to confirm first), and a drop zone for files, folders and zips (a folder's files keep their path); a layout can also carry a message and a text to show in a popup. One component in the frontend draws every plugin's tab in the style of the other Settings pages, so a plugin needs no frontend code, and a block it cannot say yet is added to that vocabulary. The tab exists while the plugin is enabled, for every user: `GET /api/plugins/tabs`, `GET /api/plugins/tab`, `POST /api/plugins/tab/action` and `POST /api/plugins/tab/upload` (a multipart form, up to 64 MB) take any signed-in user, unlike the owner-only routes above, since a tab is the user's own use of the plugin. A tab id that is not a plain slug, is one of the five Settings tabs (`behaviour`, `appearance`, `profile`, `models`, `voice`) or is already taken by another enabled plugin is left out, with a warning in the log.

## Messaging plugin type
`messaging` is a shared-provider plugin type — chat platforms as interchangeable subplugins under one generic `Plugin`/`PluginBuilder` impl. A concrete provider only ever implements `MessagingProvider` ([`src/plugins/messaging/provider.rs`](./src/plugins/messaging/provider.rs)):

```rust
trait MessagingProvider: Send + Sync + 'static {
    type Settings: Send + Sync + Clone + Serialize + DeserializeOwned + 'static;

    fn subname() -> &'static str;
    fn help_message() -> String;
    fn summary() -> &'static str;
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

Every provider shares `allowed_chat_ids`, `think` (whether a reply thinks before answering), `name_hint`, and `timezone` on top of its own settings. `name_hint`, off by default, tags every message handed to the agent with who actually sent it — `[Message from user named X with userid X, sent at ...]` — so a bot that's talking to several people (a group chat, most obviously) doesn't see them as one ongoing conversation; `timezone` (an IANA name, defaulting to UTC) is what the send time in that tag is shown in. Any images attached to an incoming message (a photo, or several sent as one album/attachment group) are forwarded to the agent the same way the frontend's own image attachments are — every image is decoded and re-encoded before it reaches the model, so a format the model server's own decoder can't handle gets caught here instead of failing the whole reply. Splitting a long reply into several platform messages ([`plugins/messaging/chunking.rs`](./src/plugins/messaging/chunking.rs)) is shared too — it only tracks blank lines and fenced code blocks, nothing platform-specific, so every provider reuses it and only implements its own last-resort hard-cut on top.

## Adding a provider
1. Add a `Settings` struct (`#[derive(ToolParams)]`, same pattern as a tool's args struct — see [TOOLS.md](./TOOLS.md)) and implement `MessagingProvider` for it.
2. Register it in [`services/bootstrap.rs`](./src/services/bootstrap.rs): add `Arc::new(MessagingProviderBuilder::<YourProvider>::new(plugin_agent.clone(), chat_store.clone()))` to the `plugin_builders` list.

That's it — HTTP routes, settings persistence, and the frontend settings form are all generic over `MessagingProvider`.

## Current plugins

### `messaging` — chat platforms
| Subname | Settings | Notes |
|---|---|---|
| `telegram` | `token` | Long-polls the Bot API. |
| `discord` | `token` | Persistent gateway connection; only responds to a `/chat` command, an @mention, or a reply to one of its own messages. |
| `vk` | `token`, `group_id` | Long-polls VK's Bots Long Poll API. The community token needs both the "Messages" and "Manage community" access rights, and the community itself needs the Bots Long Poll API — and its "Message received" event type specifically — turned on. |

Each provider's own step-by-step setup instructions are available from its settings panel in the app (or `GET /api/plugins/help`), not duplicated here.

### `coding` — structured insight into code, instead of raw file contents
A different shape of plugin from `messaging`: no shared provider trait, no background
loop, no settings — each submodule under [`plugins/coding/`](./src/plugins/coding.rs)
implements `Plugin`/`PluginBuilder` directly and contributes its own tools. A future
addition to this family (something other than dependency signatures) becomes its own
sibling submodule the same way, still under `plugin_name: "coding"`.

| Subname | Contributes | Notes |
|---|---|---|
| `signatures` | `coding.get_signatures` | Gives the model a trimmed, typed view of a dependency's public API — signatures with real types, not the full source — for rust, javascript/typescript, python, go, c/c++, java, kotlin, and csharp. No settings; enabling it needs nothing beyond the target language's own toolchain being available wherever `os.execute_command` runs. See the tool's own description (`GET /api/plugins/help?plugin_name=coding&plugin_subname=signatures`, or just ask the model) for exactly how each language resolves a dependency and what's best-effort about it — that's kept in one place (the tool description itself) rather than duplicated here and risking drift. |

## HTTP API
Registry-level management routes, under `/api/plugins`:

| Route | What it does |
|---|---|
| `GET /` | Every registered plugin, enabled and disabled alike, with its current settings and its one-line summary, by `plugin_name` then `plugin_subname`. |
| `GET /settings_schema?plugin_name=&plugin_subname=` | A plugin's settings schema, same shape as a tool's args. |
| `GET /help?plugin_name=&plugin_subname=` | A plugin's own step-by-step "how to use" message. |
| `POST /settings` | Sets (or replaces) a plugin's settings, rebuilding its live instance. |
| `POST /enable` | Enables or disables a plugin — fails if it has no settings configured yet. |
| `POST /reset_chat` | Wipes a plugin-linked chat's message history, keeping the chat and its external mapping intact — useful for retesting behavior without a stale reply from earlier still in the history the model gets replayed on every turn. |

A plugin's own instance-level routes (`Plugin::api_router()`) mount under `/api/plugins/{plugin_name}/{plugin_subname}/...` — none of the current providers expose any yet.
