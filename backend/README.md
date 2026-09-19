# Backend
Rust/axum backend for llm-tulpa. Talks to Ollama, persists chat history in Postgres, and drives the agentic tool-calling loop — deciding when a tool call needs your say-so before it runs.

## Requirements
- Rust, 2024 edition (stable, 1.85+)
- PostgreSQL (developed against 17)
- Ollama, reachable over HTTP, running a model that supports tool calling

## Running
```bash
cargo run
```
Needs Ollama up and reachable. Postgres does **not** have to be configured up front: with no database details, the backend starts in **setup mode** and serves only `/api/setup/*` and `/api/auth/login` until the frontend's first-run wizard supplies a connection (host/port/db/user/password), at which point it connects, migrates, and swaps the live services in without a restart. Tables are created automatically — nothing to migrate by hand.

Or run the whole project (frontend included) via Docker — see the repo root's `compose.yaml`.

## Database connection: config file only
The Postgres connection and the JWT secret live in a JSON config file the backend reads on startup and the setup wizard writes:
`<exe_dir>/data/settings.json` on every platform — a `data` folder next to the binary (so the directory must be writable by the user running it; under Docker that's `/usr/local/bin/data`, a named volume).

The `jwt_secret` is generated on first write. **The database connection is read from this file only — there is no environment-variable or `.env` path for it** (`DATABASE_URL`/`DATABASE_NAME` are ignored). With no `database` block in the file — or if it can't connect — the backend starts in setup mode (above), and `GET /api/setup/status` reports `configured: false`, which makes the frontend redirect every page to the setup wizard. The status check pings the database each time, so a database that goes away later sends the frontend back to the wizard as well.

## Providers and models
LLM providers and models are stored in the database: `llm_providers` (seeded with `ollama`) and `llm_models` (registered when a user picks, pulls, or switches to one). A user's default model is `user_settings.active_model_id`; **every chat is bound to a model** (`chats.model_id`, not null) — new chats start with the user's default and can be switched per chat (`POST /api/chats/model`). There is no model configured in the environment: `Agent` runs each turn against the chat's bound model, and one-shot prompts (greeting, chat naming) against the user's default. Creating a chat before any model has been registered returns 409.

## First run and accounts
The app is multi-user. The first account created (through the wizard) is the **owner**; only the owner can create or delete additional users, from the in-app Users page. There is no open self-registration. Sessions are JWTs (30-day TTL) sent as `Authorization: Bearer`; the frontend stores the token in `localStorage`. All chat/settings/file data is per-user.

## Environment variables
None of these configure the database (see above). A `.env` file next to the binary is read for them if present; real environment variables win.

| Variable | Default | What it does |
|---|---|---|
| `OLLAMA_URL` | `http://localhost:11434` | Where to reach Ollama. |
| `OLLAMA_CONTEXT_LENGTH` | `32768` | Must match whatever you set Ollama's own context window to (see `llm/`'s README) — drives the `num_predict` cap and the history-compaction thresholds. |
| `AGENT_HISTORY_LEN` | `200` | How many of a chat's most recent messages get pulled into a single turn. |
| `SEARXNG_URL` | `http://localhost:8090` | Where to reach the SearXNG instance `web.search_query` calls (the rate-limiting sidecar in front of it, not searxng's own port) — see the repo root's `searxng/`. |
| `BIND_ADDR` | `127.0.0.1:3000` | What the HTTP server binds to. Loopback-only by default; Docker overrides this to `0.0.0.0:3000` since a container's own loopback isn't reachable from outside it. |
| `RUST_LOG` | `info,sqlx::query=warn` | Standard `tracing-subscriber` filter syntax. |

## Structure
```
src/
├── main.rs      # env/config resolution, wires up services (or starts in setup mode), starts the server
├── config.rs    # the JSON config file — load/save, platform path resolution, JWT secret
├── state.rs     # AppState — Arc handles + services behind RwLock<Option> (populated once a DB is configured)
├── routes/      # HTTP layer, one folder per domain (auth/, setup/, users/ + the rest)
├── facade/      # orchestration layer between routes and services (agent, prompt)
├── services/    # backend integrations and persistence; bootstrap.rs connects + builds every
│                #   store, migrate.rs is the one centralized 3NF migration (wipe-and-recreate
│                #   on a schema-version bump), auth.rs issues/verifies JWTs
├── tools/       # every tool the model can call — see TOOLS.md
├── plugins/     # optional integrations (chat platforms, ...) — see PLUGINS.md
└── cache/       # per-user, on-demand caches for expensive-to-generate values (greeting, input examples)
```

The schema is normalized to 3NF: `users` + `user_settings` (1:1), `llm_providers` + `llm_models`, `chats` (per-user, each bound to a `llm_models` row) + `plugin_chats`, `messages` with `message_images`/`message_files` join tables, `tool_calls`, `files`, `tool_permissions`, and `user_plugins`. A single `schema_meta.version` sentinel drives the drop-and-recreate migration.

## Docs
- [TOOLS.md](./TOOLS.md) — how the tool system works, how to add a tool, and the current tool list.
- [PLUGINS.md](./PLUGINS.md) — how the plugin system works, how to add a messaging provider, and the current plugin list.
