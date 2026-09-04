# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.3] - 2026-09-05
### Added
- `os.*` extended with `get_date`, `get_process_list`, `get_network_info`, `cpu_usage`, `get_user_info`, `execute_command`, `env_read`, `env_write`. The agent's system prompt now also states the actual current date/time directly on every turn, so it doesn't assume a stale one from training.
- `storage.detect_file_type` — identifies a file by sniffing its first bytes rather than trusting its name.
- New `web.*` domain: `download_file` (save a URL straight to disk), `request` (GET/HEAD/POST/PUT/PATCH/DELETE, response read inline), and `search_query` (web search via a bundled local SearXNG instance) — see `backend/TOOLS.md`.
- New `searxng`/`searxng-nginx` Docker services backing `web.search_query`: a handful of solid search engines rather than SearXNG's full default roster, and calls are throttled at the container level so a burst of searches can't hammer the underlying engines.

### Changed
- Permission grants are now shared across every tool that needs the same kind of access, instead of each tool needing its own separate approval for the same thing — approving `storage.read_file` under a folder now also covers `storage.list_directory`/`storage.find_files`/`storage.detect_file_type` there, without a second prompt. Write and delete access remain separate, independent levels.
- `web.download_file` replaces the old `web.fetch_url` — downloads straight to disk instead of returning content inline, so it behaves the same whether the URL points at a text page, an image, or any other file type.

## [0.1.2] - 2026-08-30
### Added
- Vision — attach images to a message (frontend composer, or a photo sent through any messaging plugin) when running a vision-capable model. See the new "Vision" section in `llm/README.md` for pairing a model with a mmproj/CLIP projector.
- Messaging plugin: an optional `name_hint` setting tags every message handed to the agent with who actually sent it (name, user id, and send time, in a configurable timezone) — without it, several people talking to the same bot (e.g. a group chat) all look like one ongoing conversation.
- Telegram: multiple photos sent as one album now reach the agent as a single message with every image attached, matching how the frontend and Discord/VK already behaved.

### Changed
- The backend container now uses Docker's host network instead of its own bridge network, so it inherits whatever routing the host itself has (a VPN/proxy in particular) instead of losing it — see `compose.yaml`. Postgres and Ollama are addressed via `localhost` under this mode rather than their old Docker service names.
- The frontend's backend URL is no longer baked in at build time by default — an unset `VITE_BACKEND_URL` now falls back to whatever hostname the page itself was loaded from, so the same build works from `localhost`, a LAN IP, or anything else without a rebuild.
- Backend CORS now accepts any origin on port 5173, not just `http://localhost:5173`, to match the above.

### Fixed
- An image forwarded through a messaging plugin could make Ollama's vision decoder fail outright (`mtmd_helper_bitmap_init_from_buf: failed to decode buffer`), killing the whole reply — every image sent through a plugin is now decoded and re-encoded before it ever reaches Ollama, catching both unsupported formats and files Ollama's own decoder is unexpectedly picky about.

## [0.1.1] - 2026-08-27
### Added
- Plugin system — optional integrations, switched on/off from their own settings panel. First plugin type is messaging: talk to the agent from Telegram, Discord, or VK — see `backend/PLUGINS.md`.
- Redesigned UI — chat view, settings, tool confirmation/messages, theme preview, and message date separators.

### Fixed
- A chat that had ever been compacted sent two system messages to Ollama on every later turn, which some chat templates reject outright, breaking every subsequent turn in that chat.
- Reasoning wasn't split out from the visible answer for most replies — only worked when a literal `<think>` tag was present in the raw output, but the opening tag is injected into the prompt itself and never actually generated.

## [0.1.0] - 2026-08-18
### Added
- Chat agent backend (Rust/axum) driving a tool-calling turn loop against Ollama, with full chat history persisted in Postgres and resumable across restarts.
- Real tool-calling: `storage.*` (read/write/edit/list/find files and directories) and `os.*` (hardware, disk space) — see `backend/TOOLS.md`.
- Per-tool permission system — anything that can actually change something asks first, with the option to allow once, allow for the rest of the chat, or refuse.
- Automatic history compaction so a long or tool-heavy conversation doesn't blow the model's context window.
- React frontend with a chat UI, settings/setup flow, and three themes (dark / white / matcha-dark).
- Docker packaging for the whole stack (`compose.yaml`), with real filesystem passthrough so the agent can read/write your actual files, and files it creates keep your own ownership.
