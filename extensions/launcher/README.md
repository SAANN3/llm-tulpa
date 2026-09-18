# Launcher

A minimal floating input bar for LLM-tulpa.

## Overview

Launcher is a small, always-on-top desktop widget that opens centered on the screen. It presents a single themed input bar with a random prompt placeholder. Type a prompt, press **Enter** to open the frontend in a browser with the prompt passed as a query parameter, or press **Escape** to clear.

## Configuration

The launcher reads configuration from two sources, in order of precedence:

1. **`.env` file** — placed alongside `Cargo.toml` in the launcher directory.
2. **Environment variables** — set in the shell or system environment.

### `.env` file

Create `.env` in the launcher directory with the following:

```bash
BACKEND_URL=http://localhost:3000
FRONTEND_URL=http://localhost:5173
LAUNCHER_THEME=slate
```

Copy `.env.example` (included in the repo) as a starting point. `.env` is listed in `.gitignore` and should never be committed.

- `BACKEND_URL` — where the launcher fetches its random prompts from via `POST /api/prompts/input_examples`. Falls back to built-in prompts if the request fails or the response is empty.
- `FRONTEND_URL` — the frontend's base URL. The launcher opens `<FRONTEND_URL>?prompt=<encoded_input>` in a browser when you press **Enter**.
- `LAUNCHER_THEME` — which built-in theme to use. Valid values: `slate` (dark), `paper` (light), or `matcha` (matcha-dark). Defaults to `slate` if unset or unrecognized.

### Environment variable fallback

You can also set these in the shell instead of using `.env`:

```bash
BACKEND_URL=http://localhost:3000 FRONTEND_URL=http://localhost:5173 LAUNCHER_THEME=paper ./launcher
```

## Themes

Three built-in themes, controlled via the `LAUNCHER_THEME` environment variable:

| LAUNCHER_THEME | Background | Foreground  |
|----------------|------------|-------------|
| `slate`        | `#0f172a`  | `#cbd5e1`   |
| `paper`        | `#f4f6f8`  | `#16181d`   |
| `matcha`       | `#1d1e18`  | `#9fbb9f`   |

## Build

```bash
cargo build --release
```

## Behavior

- Opens centered on screen, always on top, undecorated.
- Fixed at ~620 × 56 px; not resizable.
- Pressing **Enter** with text launches `xdg-open` pointing at `http://localhost:5173/?prompt=<encoded_input>`.
- Pressing **Escape** clears the input and keeps the launcher open.
- The app exits immediately after opening the browser (single-shot launcher).
