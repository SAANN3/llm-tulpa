# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.4] - 2026-09-13
### Added
- File attachments: upload any file type to a chat (drag-and-drop, or the composer's picker), with previews for PDFs, Office documents, spreadsheets/CSV (rendered as a table), syntax-highlighted code, and plain text (also the fallback for any other extension whose content isn't binary). The model can read an attached file's actual content on request (`files.get_attached_file`), and can attach a file of its own to a reply (`ui.attach_file`) — landing on the actual message it prints, not an intermediate tool-calling one with nothing in it.
- `llm.read_image` — a tool that makes its own one-shot vision call on an image given by path, for an image the model found or was pointed to rather than one already visible to it inline in the conversation.
- Auto-confirm mode (Settings and Setup) — a local, per-browser toggle that resolves tool permission prompts automatically instead of showing them, for letting the agent run a task unsupervised.
- The backend's own container now permanently ships python3 (+pip, +venv), nodejs (+npm), go, rustc (+cargo), build-essential, git, jq, unzip/zip, curl, wget, poppler-utils, ripgrep, fd, tree, sqlite3, docx2txt, gnumeric, and Playwright + a real headless Chromium — plus a scoped, passwordless `sudo` covering just `apt-get`/`apt`/`dpkg`, so `os.execute_command` can install anything else on top freely (temporarily — only what's in this list survives a container restart).
- GUI passthrough — the host's display-server socket and `DISPLAY` are now passed into the backend container, so a GUI app launched non-headless (e.g. Playwright's Chromium) shows up as a real window on the host's own screen instead of only ever running headless.
- The system prompt now states its own deployment context up front — a private, self-hosted, single-user instance the user themselves builds and operates.
- `AGENTS.md` — onboarding for an AI coding agent working in this repo (how to run it, how to verify a change, doc/comment style, where each doc lives).

### Changed
- Ollama's KV cache can now be quantized (`OLLAMA_KV_CACHE_TYPE=q8_0` + `OLLAMA_FLASH_ATTENTION=1`) to trade some precision for a larger usable context window on VRAM-constrained hardware — opt-in via a deployment's own `llm/.env`, no default in the shared compose file. See `llm/README.md`.

### Fixed
- `OLLAMA_CONTEXT_LENGTH` could silently disagree between the backend and Ollama itself, since each read its own separate `.env` file — a single oversized tool result could then exceed what Ollama could actually accept, entirely silently (no log on the backend side, no error surfaced in the UI). Now sourced from one root `.env` for both; an Ollama request failure is logged with its actual status/body, and a failed turn now shows a visible, dismissible error in the chat instead of the "thinking" indicator just disappearing.
- `web.request` returned an HTML response as raw markup rather than its actual readable content — now converted via `html2text` before the size cap applies; its description also now points at the baked-in Playwright for anything JS-rendered, interactive, or screenshot-worthy.
- `storage.list_directory` had no cap at all, so a large enough directory's listing alone could exceed the model's context budget in one call — now returns at most 200 entries per call (sorted by name), with an `offset` argument to page through the rest.
- `web.search_query`'s `bing` engine was returning results with nothing to do with the actual query (almost certainly Bing's own scraper-hostility blocking SearXNG's engine) — replaced with `yandex`.
- `os.execute_command`'s destructive-command blocklist matched `dd`/`mkfs` as a raw substring anywhere in the command text, so unrelated text merely containing e.g. "sed dd od" as a list of tool names got permanently blocked with nothing to approve. Now only matches them as an actual leading word of a shell statement.
- A `LazyList` (the chat sidebar, message history) could end up scrolled to the wrong position after loading — its `jumpToTop`/`jumpToBottom` didn't update the list's own internal scroll anchor, so the very next unrelated render could drag the scroll position back to a stale computed value.
- A browser notification fired with an empty body when a reply's content came back blank (e.g. the model exhausted its output budget before producing an answer).

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
