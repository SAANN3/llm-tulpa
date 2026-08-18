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
Needs Postgres and Ollama already up and reachable — point at them with the env vars below. Tables/columns are created automatically on first run, nothing to migrate by hand.

Or run the whole project (frontend included) via Docker — see the repo root's `compose.yaml`.

## Environment variables
| Variable | Default | What it does |
|---|---|---|
| `OLLAMA_URL` | `http://localhost:11434` | Where to reach Ollama. |
| `OLLAMA_MODEL_NAME` | `local-llm` | The model tag to call. |
| `OLLAMA_CONTEXT_LENGTH` | `32768` | Must match whatever you set Ollama's own context window to (see `llm/`'s README) — drives the `num_predict` cap and the history-compaction thresholds. |
| `DATABASE_URL` | `postgres://postgres:postgres@localhost:5432` | Postgres connection string, without a database name. |
| `DATABASE_NAME` | `llm_tulpa` | Database name — created automatically if it doesn't exist. |
| `AGENT_HISTORY_LEN` | `200` | How many of a chat's most recent messages get pulled into a single turn. |
| `BIND_ADDR` | `127.0.0.1:3000` | What the HTTP server binds to. Loopback-only by default; Docker overrides this to `0.0.0.0:3000` since a container's own loopback isn't reachable from outside it. |
| `RUST_LOG` | `info,sqlx::query=warn` | Standard `tracing-subscriber` filter syntax. |

## Structure
```
src/
├── main.rs      # env vars, wires up every service, starts the server
├── state.rs     # AppState — the shared Arc handles every route pulls from
├── routes/      # HTTP layer, one folder per domain
├── facade/      # orchestration layer between routes and services
├── services/    # backend integrations and persistence
├── tools/       # every tool the model can call — see TOOLS.md
└── cache/       # background-refreshed caches for expensive-to-generate values
```

## Docs
- [TOOLS.md](./TOOLS.md) — how the tool system works, how to add a tool, and the current tool list.
