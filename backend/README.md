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

## Configuration: `data/settings.json`
Everything the backend is configured with lives in one JSON file it reads on startup: `<exe_dir>/data/settings.json` on every platform — a `data` folder next to the binary (so the directory must be writable by the user running it; natively that's `target/debug/data/` or `target/release/data/`, under Docker it's `backend/data/`, bind-mounted). There are no environment variables or `.env` files for the backend; the only environment variable that configures it is `RUST_LOG` (standard `tracing-subscriber` filter syntax, default `info,sqlx::query=warn`).

The setup wizard writes the `database` block and the app generates `jwt_secret` on first run; the rest you edit by hand. Every field has a default, so a file listing only what differs is fine. Copy [`data/settings.example.json`](./data/settings.example.json) (native) or [`data/settings.docker.example.json`](./data/settings.docker.example.json) (what `start-docker.sh` copies for you) to `data/settings.json` to start from a complete list. `settings.json` itself is gitignored — it holds the JWT secret and the database password.

| Field | Default | What it does |
|---|---|---|
| `bind_addr` | `127.0.0.1:3000` | What the HTTP server binds to. Loopback-only by default; `0.0.0.0:3000` makes it reachable from other machines. |
| `ollama.url` | `http://localhost:11434` | Where to reach Ollama. |
| `ollama.context_length` | `32768` | Must match Ollama's own context window (`OLLAMA_CONTEXT_LENGTH` in the root `.env` — see `llm/README.md`) — drives the per-request `num_predict` cap (sized from what is left of the window after the prompt) and the history-compaction thresholds. The two live in different files, so change them together. |
| `agent_history_len` | `200` | How many of a chat's most recent messages get pulled into a single turn. |
| `files_dir` | `~/.llm-tulpa/files` | Where uploaded files are stored. |
| `jobs_dir` | `~/.llm-tulpa/jobs` | Where `os.start_job`'s background jobs write their log files. |
| `job_log_retention_days` | `7` | How many days a *finished* job's log is kept. The sweep runs once at startup and deletes older logs (the job's row stays, marked as cleaned up); `0` keeps every log. |
| `searxng_url` | `http://localhost:8090` | The SearXNG instance `web.search_query` calls (the rate-limiting sidecar in front of it, not SearXNG's own port) — see the repo root's `searxng/`. |
| `host_root` | unset | Where the host's root filesystem is mounted inside a container, so `os.get_disk_space` reports the host's disk. Docker: `/hostfs`. |
| `model_dir` | unset | The folder of local `.gguf` model files — the same one Ollama's `MODEL_DIR` points at. Unset turns importing local model files off. Docker: `/models`. |
| `database` | unset | Written by the setup wizard: `host`, `port`, `name`, `user`, `password`. |
| `jwt_secret` | generated | Signs sessions. Generated and saved when missing. |

A file that exists but doesn't parse stops the backend with the parse error rather than being replaced — silently starting over would forget the database connection and log everyone out.

With no `database` block — or if it can't connect — the backend starts in **setup mode**, and `GET /api/setup/status` reports `configured: false`, which makes the frontend redirect every page to the setup wizard. The status check pings the database and, if a database *is* configured but the connection never came up (Postgres wasn't ready at boot), retries it — so a slow Postgres doesn't leave the app stuck until a restart. `POST /api/setup/database` only works while no database has been configured; changing it later means editing `settings.json`, since an unauthenticated route mustn't be able to repoint the app.

## Providers and models
LLM providers and models are stored in the database: `llm_providers` (seeded with `ollama`) and `llm_models` (registered when a user picks, pulls, or switches to one). A user's default model is `user_settings.active_model_id`; **every chat is bound to a model** (`chats.model_id`, not null) — new chats start with the user's default and can be switched per chat (`POST /api/chats/model`). There is no model configured in the environment: `Agent` runs each turn against the chat's bound model, and one-shot prompts (greeting, chat naming) against the user's default. Creating a chat before any model has been registered returns 409.

### Getting models: pull, Hugging Face, or your own `.gguf` files
The owner manages models from the model picker (or the `/api/llm/*` endpoints); everyone else can only pick among what's installed.
- **Pull** a library model (`qwen3:8b`) or a Hugging Face GGUF (`hf.co/<user>/<repo>[:<quant>]` — Ollama pulls those natively) with `POST /api/llm/pull`. `GET /api/llm/catalog` is Ollama's public library as JSON, parsed on the backend from ollama.com's page (a short built-in list is returned, with `live: false`, when it can't be reached).
- **Import local files** without downloading anything: with `model_dir` set, `GET /api/llm/local_files` lists the `.gguf` files under it (up to three folders deep) and `POST /api/llm/import` turns any number of them into Ollama models — each file is hashed, uploaded to Ollama as a blob, and created (what `ollama create` does). What a file *is* comes from its own GGUF header (`services/gguf.rs`), not its name: a language model, a vision projector (`mmproj`, architecture `clip`), or invalid (empty or not GGUF). A projector can be paired with a model to give it vision, and the pairing is checked: the projector's output size must equal the model's hidden size, so a projector from another model family is refused with a clear 400 instead of producing garbage on images. The listing gives every model its compatible projectors and, when one clearly stands out (same author-given name, or its file name is the model's up to the quantization, or it's the only fit), a suggestion the picker preselects. Ollama then keeps its own copy in its data directory, exactly as `ollama create` does.
- Pulls and imports run **in the background** (`202` + a task); `GET /api/llm/tasks` reports progress and results for running tasks and those finished in the last hour.
A model must be installed before a chat or a user's default can be bound to it.

## First run and accounts
The app is multi-user. The first account created (through the wizard) is the **owner**; only the owner can create or delete additional users, from the in-app Users page. There is no open self-registration. Sessions are JWTs (30-day TTL) sent as `Authorization: Bearer`; the frontend stores the token in `localStorage`. All chat/settings/file data is per-user.

## Structure
```
src/
├── main.rs      # loads the config, wires up services (or starts in setup mode), starts the server
├── config.rs    # settings.json — every backend setting, load/save, platform path resolution, JWT secret
├── state.rs     # AppState — Arc handles + services behind RwLock<Option> (populated once a DB is configured)
├── routes/      # HTTP layer, one folder per domain (auth/, setup/, users/ + the rest)
├── facade/      # orchestration layer between routes and services (agent, prompt)
├── services/    # backend integrations and persistence; bootstrap.rs connects + builds every
│                #   store, migrate.rs is the one centralized 3NF migration, auth.rs issues/verifies
│                #   JWTs, model_library.rs is the model catalog + local files + pull/import tasks
├── tools/       # every tool the model can call — see TOOLS.md
├── plugins/     # optional integrations (chat platforms, ...) — see PLUGINS.md
└── cache/       # per-user, on-demand caches for expensive-to-generate values (greeting, input examples)
```

The schema is normalized to 3NF: `users` + `user_settings` (1:1), `llm_providers` + `llm_models`, `chats` (per-user, each bound to a `llm_models` row) + `plugin_chats`, `messages` with `message_images`/`message_files` join tables, `tool_calls`, `files`, `tool_permissions`, and `user_plugins`. A single `schema_meta.version` sentinel drives the migration. Moving between versions never wipes data: a database *newer* than the build is refused untouched, and the one-time upgrade from the pre-accounts (single-user) release keeps everything — see below.

## Docs
- [TOOLS.md](./TOOLS.md) — how the tool system works, how to add a tool, and the current tool list.
- [PLUGINS.md](./PLUGINS.md) — how the plugin system works, how to add a messaging provider, and the current plugin list.

## Upgrading from the single-user release
A database created before accounts existed (chats without a `user_id`) is **kept, not wiped**. On the first start its tables are moved, untouched, into a `legacy` Postgres schema and the new schema is created beside them. When the owner account is created (setup wizard), everything is copied in for them in one transaction: chats (ids preserved, so uploaded files and references stay valid), messages with their images, attached files and tool calls, permission grants, plugin settings, the old settings row (name, timezone, notifications), and a default model — those chats ran the model `llm/start.sh` served as `local-llm`, so they're bound to it. The old tables stay as a backup in a `legacy_adopted_<timestamp>` schema; drop it (`DROP SCHEMA legacy_adopted_… CASCADE`) once you're satisfied. If the owner already exists (an interrupted import), it's retried on startup.
