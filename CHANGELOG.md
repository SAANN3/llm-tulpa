# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
