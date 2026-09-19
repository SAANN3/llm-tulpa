# For AI coding agents working in this repo

llm-tulpa is a local-first LLM chat agent with real tool-calling: Rust/axum backend, React/TS frontend, Postgres, Ollama for inference, everything Docker-packaged. See [`README.md`](./README.md) for the user-facing overview — this file is about working *on* the codebase itself.

## Running it
```bash
HOST_UID=$(id -u) HOST_GID=$(id -g "$(whoami)") docker compose up -d --build
```
`HOST_UID`/`HOST_GID` matter: the backend container runs as that exact user (not root), so files `storage.*` tools write on the host keep the real user's ownership, and a scoped `sudo` for package installs is baked into the image for that same uid. After changing backend or frontend code, rebuild just that service rather than the whole stack: `docker compose up -d --build backend` (or `frontend`). Backend/frontend can also run directly without Docker — `cargo run` / `npm run dev` — see their own `README.md`s for env vars.

Two separate `.env` files exist and don't share scope, which is a real, easy-to-hit gotcha: the root `.env` (backend + shared config, e.g. `OLLAMA_CONTEXT_LENGTH`) and `llm/.env` (Ollama-service-only config, e.g. `MODEL_FILE`, the KV-cache quantization opt-in). A setting only one of them reads has to actually live in that one — see `compose.yaml`'s and `llm/compose.yaml`'s own comments for which is which.

## Verify before claiming something works
This codebase has a strong, established live-verification habit — don't skip it:
- A backend change: a temporary `#[cfg(test)] mod temp_verify` block, run with `cargo test --bin backend` against the real (already-running) Postgres, deleted once it passes. Not a permanent test suite — a throwaway check for this one change.
- A frontend change: actually drive it (Playwright against the real running dev server, or the deployed container) rather than asserting from reading the code. Screenshot and look at it for anything visual.
- A tool description change: consider whether the model would actually see and act on it correctly — test with a real prompt through `/agent/chat` if the change is behavior-affecting, not just cosmetic.
Several real, non-obvious bugs in this project were only ever caught this way, not by review.

## Docs map — keep these in sync with what you change
- [`backend/TOOLS.md`](./backend/TOOLS.md) — every tool, the permission model, background jobs and the server-event stream, how to add a tool. Update it when adding/changing a tool.
- [`extensions/launcher/README.md`](./extensions/launcher/README.md) — the desktop quick-launcher (a separate crate, built and run on its own, not part of the Docker stack).
- [`backend/PLUGINS.md`](./backend/PLUGINS.md) — the plugin system (Telegram/Discord/VK today). Update it when touching plugin behavior.
- [`frontend/THEMING.md`](./frontend/THEMING.md) — how theme variants/colors work.
- [`llm/README.md`](./llm/README.md) — swapping models, context window, KV-cache quantization tradeoff.
- [`CHANGELOG.md`](./CHANGELOG.md) — every user-visible change, at release time. See the style note below before writing an entry.

## Writing docs and comments: no diff language against something that never shipped
Describe current behavior, not "now does X instead of Y" — *unless* Y was a real, previously-released behavior a user could have actually seen. Comparing against an intermediate state that only ever existed mid-development (this session's own iteration, never tagged/released) is meaningless to anyone who wasn't watching that session happen — from their side, it's just a new feature, full stop. This applies to rustdoc/JSDoc comments and `CHANGELOG.md`/`README.md` alike. (A `CHANGELOG.md` "Fixed"/"Changed" entry is the one place real version-to-version diffing belongs — that's the whole point of those sections — as long as the "before" state was actually shipped.)

## System prompt (`SYSTEM_PROMPT` in `backend/src/facade/agent.rs`)
Word a behavioral rule as a check to run ("before doing X, check Y"), not a capability fact ("you can/can't do X"). A fact goes stale the moment something changes and causes a false refusal; a check stays correct regardless. Keep the array in sync with `backend/src/tools/os/execute_command.rs`'s own baked-tool-list text when either changes — they're meant to describe the same reality.

## Git
Never add attribution lines (`Co-Authored-By`, a session/tool URL, etc.) to a commit message or PR description, in this repo, ever.
