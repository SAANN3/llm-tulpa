# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]
### Added
- Graduated thinking-effort control — when the active model's own chat template supports a set of effort levels (e.g. `xhigh`/`medium`/`low`), the composer now offers them in a selector next to the existing thinking on/off toggle, and the chosen level is sent verbatim as `think` instead of a plain bool; the toggle itself still works exactly as before for any model. Which levels are offered is discovered live per composer mount from the model's own chat template (`GET /api/llm/thinking_capability` — a new endpoint that reads the template via Ollama's `/api/show` and extracts the exact strings the template's `reasoning_effort` check accepts, in the order it lists them), not hardcoded per model family and not cached anywhere: the active model can change without a restart, the call reads stored model metadata with no model load required, and a template with no recognizable thinking markers at all degrades to no control rather than one that would do nothing. Plain `true`/`false` behavior is preserved exactly for every call site that has no UI behind it (compaction, `llm.read_image`, messaging plugins, chat naming).
- Media previews for file attachments — image, video, and audio files attached by id (including files the model attached to its own reply via `ui.attach_file`) now render in the preview popup via the same `<img>`/`<video>`/`<audio>` elements as everything else (an embedded media element loads a resource to render, so the download route's `Content-Disposition: attachment` never fires for them), and the small attachment chip shows a real thumbnail instead of a generic file icon when its file is actually an image or video.
- The `mtp` compose profile (`llm/compose.yaml`) — an optional raw llama.cpp server build with Multi-Token Prediction speculative decoding (`llama-mtp`) plus a small Ollama-to-OpenAI wire-format translator sidecar (`mtp-proxy`), since Ollama 0.34.0 has no `--spec-type`/MTP flag at all and silently ignores the MTP draft-head tensors some of our GGUFs already carry. Whether MTP helps turns out to be genuinely task-dependent — a clean, short-prompt benchmark and real, conversation-depth usage through the actual chat API don't necessarily agree — so it's meant for occasional per-task A/B rather than day-to-day use: it's kept fully configured and buildable but out of a plain `docker compose up -d` behind a named compose profile, so it doesn't load a model into the same card as `ollama` at once. `llama-mtp`'s startup flags mirror the Ollama service's KV-cache quantization, flash-attention, context length, and mmproj pairing so a comparison doesn't also compare different settings; the backend's `OLLAMA_URL` pointed at `:11435` is all it takes to switch.
- Key facts — a durable companion to the compaction summary (`Chat.key_facts`: an optional goal plus a list of short facts). Where the narrative summary gets regenerated from itself on every fold and can gradually compress away specific detail, key facts are only ever appended to, never rewritten: each fold makes a separate, focused extraction call (existing facts shown only as "don't repeat this" context, never fed back in for the model to re-derive), and merging the result into what's already stored is a deterministic Rust function, not a model rewrite. Shown to the model as a distinct "Key facts (durable...)" block ahead of the narrative summary once a chat has any.

### Fixed
- `os.execute_command` returned stdout and stderr uncapped, so a single oversized command output stored as a chat message could blow the model's context window on the next turn — Ollama then silently truncated the oversized history down to fit (no error anywhere in the pipeline, and that path shipped with zero logging), which excluded nearly the entire actual conversation, so the model's very next reply looked like it had genuinely forgotten everything. Stdout and stderr are now each capped independently at 40,000 characters — the same cap-and-truncation-marker convention as `storage.read_file` — and the tool's own description tells the model the cap exists and how to work around it for genuinely large output (narrower searches, byte-bounded pipes rather than line-capped ones), since that's exactly the kind of thing the model needs to know to not re-trigger the same failure.
- The `Select` dropdown panel (the thinking-mode selector's container, but any `Select`) always opened downward, so a trigger near the bottom of the viewport — the chat composer is exactly that, and it's where this control lives — clipped the panel off-screen and made every option past the first unreachable. It now measures the actual space below the trigger when it opens (not at mount — the trigger's position can change between opens) and opens upward when there isn't enough room below and more room above.
- Ollama's prompt cache was failing to match on almost every turn — llama.cpp's own logs showed as little as 3 tokens of a 47,000+ token prompt matching anything cached, forcing a full reprocess of the entire conversation history instead of just what's actually new, and turning prefill deep into a conversation into most of a turn's wall time (tens of seconds, sometimes over a minute, dwarfing actual generation). Two independent causes, both in what gets sent as the prompt's very first tokens (where the cache's prefix match starts, so any change there invalidates everything after it, no matter how much of the rest is identical to the last call): the system prompt embedded a fresh to-the-second timestamp on every single call, and each tool's parameter schema serialized from a freshly built `HashMap`, whose randomized per-instance iteration order changed the schema's literal text on every call even with identical tool definitions. Fixed by moving the timestamp onto whichever message is newest each turn (the new user turn, or a tool result — content that was never going to be cached anyway) instead of the static system prompt, and by switching the tool-schema map to a `BTreeMap` for deterministic ordering. Live-verified against a 275,000-character stress conversation: steady-state turns now reuse 90%+ of the cached prefix and complete in ~4-5s instead of ~40-70s at the same real context depth.

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
