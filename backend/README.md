# Backend
Rust/axum backend for llm-tulpa. Runs and talks to llama.cpp (or Ollama), persists chat history in Postgres, and drives the agentic tool-calling loop — deciding when a tool call needs your say-so before it runs.

## Requirements
- Rust, 2024 edition (stable, 1.85+)
- PostgreSQL (developed against 17)
- A model that supports tool calling, run by llama.cpp — the backend downloads and starts it itself (the setup wizard's llama.cpp step, or `llama_cpp.dir` pointing at your own build) — or by an Ollama reachable over HTTP

## Running
```bash
cargo run
```
Needs a model server only once you chat: the backend starts its own `llama-server` on the first request that needs a model. Postgres does **not** have to be configured up front: with no database details, the backend starts in **setup mode** and serves only `/api/setup/*` and `/api/auth/login` until the frontend's first-run wizard supplies a connection (host/port/db/user/password), at which point it connects, migrates, and swaps the live services in without a restart. Tables are created automatically — nothing to migrate by hand.

Or run the whole project (frontend included) via Docker — see the repo root's `compose.yaml`.

## Configuration: `data/settings.json`
Everything the backend is configured with lives in one JSON file it reads on startup: `<exe_dir>/data/settings.json` on every platform — a `data` folder next to the binary (so the directory must be writable by the user running it; natively that's `target/debug/data/` or `target/release/data/`, under Docker it's `backend/data/`, bind-mounted). There are no environment variables or `.env` files for the backend; the only environment variable that configures it is `RUST_LOG` (standard `tracing-subscriber` filter syntax, default `info,sqlx::query=warn`).

The setup wizard writes the `database` block and the app generates `jwt_secret` on first run; the rest you edit by hand. Every field has a default, so a file listing only what differs is fine. Copy [`data/settings.example.json`](./data/settings.example.json) (native) or [`data/settings.docker.example.json`](./data/settings.docker.example.json) (what `start-docker.sh` copies for you) to `data/settings.json` to start from a complete list. `settings.json` itself is gitignored — it holds the JWT secret and the database password.

| Field | Default | What it does |
|---|---|---|
| `bind_addr` | `127.0.0.1:3000` | What the HTTP server binds to. Loopback-only by default; `0.0.0.0:3000` makes it reachable from other machines. |
| `ollama.url` | `http://localhost:11434` | Where to reach Ollama. The owner can change it in the app (the wizard's Ollama step or the Models page's Ollama tab, with a Test button); it applies at once and is saved here (`GET`/`POST /api/llm/ollama`, `POST /api/llm/ollama/test`). |
| `llama_cpp.dir` | unset | The folder holding the llama.cpp release (`llama-server` and its libraries). Unset: `~/.llm-tulpa/llama`, where the setup step downloads it; inside the Docker image `~/.llm-tulpa/llama-docker`, because its home is yours and a container's Vulkan build must not replace the ROCm or CUDA one a native backend uses. |
| `llama_cpp.port` | `18080` | The port the backend's own `llama-server` listens on (loopback only). |
| `llama_cpp.autostart` | `false` | Load the default model when the backend starts. Off: it loads on the first request. |
| `llama_cpp.idle_unload_minutes` | `0` | Stop the server after this many idle minutes, freeing the GPU for something else. `0` never stops it. |
| `llama_cpp.load_timeout_secs` | `300` | How long to wait for a model to finish loading. |
| `llama_cpp.external_url` | unset | Use a `llama-server` already running at this URL instead of starting one; the backend then neither starts nor stops it, so the model can't be switched from the UI. |
| `ollama.context_length` | `32768` | Must match Ollama's own context window (`OLLAMA_CONTEXT_LENGTH` in the root `.env` — see `llm/README.md`) — drives the per-request `num_predict` cap (sized from what is left of the window after the prompt) and the history-compaction thresholds. The two live in different files, so change them together. |
| `agent_history_len` | `2000` | How many of a chat's most recent messages get pulled into a single turn. Keep it high: once a chat has more messages than this, the oldest one drops out on every new message, which changes the prompt from its start and forces a full re-evaluation each call; history is bounded by compaction (by tokens), not by this. |
| `files_dir` | `~/.llm-tulpa/files` | Where uploaded files are stored. |
| `jobs_dir` | `~/.llm-tulpa/jobs` | Where `os.start_job`'s background jobs write their log files. |
| `job_log_retention_days` | `7` | How many days a *finished* job's log is kept. The sweep runs once at startup and deletes older logs (the job's row stays, marked as cleaned up); `0` keeps every log. |
| `searxng_url` | `http://localhost:8090` | The SearXNG instance `web.search_query` calls (the rate-limiting sidecar in front of it, not SearXNG's own port) — see the repo root's `searxng/`. |
| `host_root` | unset | Where the host's root filesystem is mounted inside a container, so `os.get_disk_space` reports the host's disk. Docker: `/hostfs`. |
| `model_dir` | unset | The folder of local `.gguf` model files (the owner can also choose it in the app, which writes it here and applies it without a restart) — the models llama.cpp runs, where Hugging Face downloads land, and the folder Ollama's `MODEL_DIR` points at. Unset turns running and importing local model files off. Docker: `/models`. |
| `database` | unset | Written by the setup wizard: `host`, `port`, `name`, `user`, `password`. |
| `jwt_secret` | generated | Signs sessions. Generated and saved when missing. |

A file that exists but doesn't parse stops the backend with the parse error rather than being replaced — silently starting over would forget the database connection and log everyone out.

With no `database` block — or if it can't connect — the backend starts in **setup mode**, and `GET /api/setup/status` reports `configured: false`, which makes the frontend redirect every page to the setup wizard. The status check pings the database and, if a database *is* configured but the connection never came up (Postgres wasn't ready at boot), retries it — so a slow Postgres doesn't leave the app stuck until a restart. `POST /api/setup/database` only works while no database has been configured; changing it later means editing `settings.json`, since an unauthenticated route mustn't be able to repoint the app.

## Providers and models
LLM providers and models are stored in the database: `llm_providers` (seeded with `ollama` and `llama-cpp`, the default) and `llm_models` (registered when a user picks, pulls, adds, or switches to one). A user's default model is `user_settings.active_model_id`; **every chat is bound to a model** (`chats.model_id`, not null) — new chats start with the user's default and can be switched per chat (`POST /api/chats/model`). There is no model configured in the environment: `Agent` runs each turn against the chat's bound model, and one-shot prompts (greeting, chat naming) against the user's default. Creating a chat before any model has been registered returns 409.

### The managed llama.cpp (`llama-cpp`)
`services/llama_runtime.rs` runs `llama-server` as a child process: it starts on the first request that needs a model (or at boot with `llama_cpp.autostart`), switches between **launch profiles** (`launch_profiles`: context or automatic, KV cache types, flash attention, GPU layers, MTP drafting, vision projector and whether it runs on the GPU, extra arguments), unloads after `llama_cpp.idle_unload_minutes` idle minutes and stops with the backend. One server holds one model: a request needing another profile queues behind the turn, greeting or summary in progress (a `423` only after 15 minutes), a turn keeps the model it started on until its final reply, and every state change is a `model_state` event on `GET /api/events`. Ollama and llama.cpp never hold the GPU together: before llama.cpp loads, Ollama is asked to unload, and a turn on an Ollama model first stops an idle llama-server.

- `/api/runtime`: status (state, loaded profile, who is using it, the queue, where the model went), `load`, `stop` (owner), `logs` (owner), `devices` (`llama-server --list-devices`), `test` (a prompt through a profile: tokens per second, share of MTP drafts accepted), `models` (the registered `.gguf` files; the owner registers one, which pairs its projector and detects an MTP head).
- `GET`/`POST /api/runtime/server` (owner): how long an idle model stays loaded (`idle_unload_minutes`, 0 never), whether the default model loads at startup (`autostart`) and the load timeout; the idle time and the timeout apply at once and all three are written to `settings.json`.
- `/api/runtime/hardware`, `install` (owner): what the machine has and the llama.cpp build that suits it; installing downloads the release this version was tested with (sha256 checked), the newest one, or records your own binary, as a background task. Only a folder the installer made is ever replaced.
- `/api/profiles` (the owner writes, everyone reads) and `/api/presets` (per user: sampling presets, one chosen per model, templates, export/import). The chosen preset is sent with every call to that model.
- `/api/hf` search, files, download (owner) and tasks: Hugging Face GGUF downloads into `model_dir` as background tasks that resume and check the published sha256; gated repositories need the user's token (`hf_token`, never sent back).
- `POST /api/runtime/setup-complete` and `rebind-chats` (owner) back the wizard's "setup changed" flow for an install that predates the llama.cpp step.

### Getting Ollama models: pull, Hugging Face, or your own `.gguf` files
`GET /api/llm/models?provider=` answers for one provider (by default the caller's default model's): what Ollama has installed, or the registered llama.cpp model files. The rest of this section is Ollama's. The owner manages them from the Models page's Ollama tab (or the `/api/llm/*` endpoints); everyone else can only pick among what's installed.
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
├── facade/      # orchestration layer between routes and services (agent, prompt, stats)
├── services/    # backend integrations and persistence; bootstrap.rs connects + builds every
│                #   store, migrate.rs is the one centralized 3NF migration, auth.rs issues/verifies
│                #   JWTs, model_library.rs is the model catalog + local files + pull/import tasks,
│                #   llm/ the model providers (llama_cpp.rs, ollama.rs behind one trait), llama_runtime.rs
│                #   runs llama-server, llama_install.rs installs it, hf_library.rs downloads from Hugging Face
├── tools/       # every tool the model can call — see TOOLS.md
├── plugins/     # optional integrations (chat platforms, ...) — see PLUGINS.md
└── cache/       # per-user, on-demand caches for expensive-to-generate values (greeting, input examples)
```

The schema is normalized to 3NF: `users` + `user_settings` (1:1), `llm_providers` + `llm_models` + `launch_profiles`, `sampling_presets` + `user_preset_choice`, `chats` (per-user, each bound to a `llm_models` row; a sub-agent's chat points at the chat that started it through `parent_chat_id`) + `plugin_chats`, `messages` with `message_images`/`message_files` join tables, `tool_calls`, `files`, `jobs` (background commands and sub-agents), `tool_permissions`, and `user_plugins`. A single `schema_meta.version` sentinel drives the migration. Moving between versions never wipes data: a database *newer* than the build is refused untouched, and the one-time upgrade from the pre-accounts (single-user) release keeps everything — see below.

## Docs
- [TOOLS.md](./TOOLS.md) — how the tool system works, how to add a tool, and the current tool list.
- [PLUGINS.md](./PLUGINS.md) — how the plugin system works, how to add a messaging provider, and the current plugin list.

## Upgrading from the single-user release
A database created before accounts existed (chats without a `user_id`) is **kept, not wiped**. On the first start its tables are moved, untouched, into a `legacy` Postgres schema and the new schema is created beside them. When the owner account is created (setup wizard), everything is copied in for them in one transaction: chats (ids preserved, so uploaded files and references stay valid), messages with their images, attached files and tool calls, permission grants, plugin settings, the old settings row (name, timezone, notifications), and — from a database made by a development build — each chat's compaction key facts and its background-job records, and a default model — those chats ran the model `llm/start.sh` served as `local-llm`, so they're bound to it. The old tables stay as a backup in a `legacy_adopted_<timestamp>` schema; drop it (`DROP SCHEMA legacy_adopted_… CASCADE`) once you're satisfied. If the owner already exists (an interrupted import), it's retried on startup.
