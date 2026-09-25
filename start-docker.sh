#!/bin/sh
set -e
cd "$(dirname "$0")"

# The backend reads all of its settings from backend/data/settings.json (see its README). A
# fresh checkout doesn't have one yet — start from the Docker-ready example, which the backend
# then fills in (JWT secret, database) as the setup wizard runs.
if [ ! -f backend/data/settings.json ]; then
  cp backend/data/settings.docker.example.json backend/data/settings.json
fi

# `ollama` isn't behind a profile (see llm/compose.yaml) — it's the zero-setup
# default and always starts, no `.env` required. `mtp` is the opt-in alternative
# (COMPOSE_PROFILES=mtp in .env, see .env.example): since profiles are additive,
# not exclusive, requesting it wouldn't stop `ollama` from *also* starting, and
# loading two models onto the same GPU at once is the VRAM contention this
# project has hit before — so when `mtp` is active, `ollama` is explicitly
# scaled to zero (and any leftover container from a previous, different choice
# stopped) instead of just left to collide with it.
ACTIVE_PROFILE=$(grep -E '^COMPOSE_PROFILES=' .env 2>/dev/null | cut -d'=' -f2 | tr -d ' "')

# Points settings.json's ollama.url at whichever backend ACTIVE_PROFILE picked, so
# switching really is the one COMPOSE_PROFILES edit — but only between the two
# addresses this script itself manages (11434 ollama, 11435 mtp-proxy); a URL
# already pointed somewhere else (a remote Ollama, a non-default port) is left
# alone rather than silently overwritten. The backend only reads this at process
# startup (main.rs, no hot reload), so an already-running backend needs an
# explicit restart to actually pick up a change here — done below, and only when
# the URL actually changed, so an unrelated rerun doesn't interrupt a live chat.
BEFORE_URL=$(grep -o '"url": "[^"]*"' backend/data/settings.json)
if [ "$ACTIVE_PROFILE" = "mtp" ]; then
  sed -i 's|"url": "http://localhost:11434"|"url": "http://localhost:11435"|' backend/data/settings.json
else
  sed -i 's|"url": "http://localhost:11435"|"url": "http://localhost:11434"|' backend/data/settings.json
fi
AFTER_URL=$(grep -o '"url": "[^"]*"' backend/data/settings.json)

# --build is cheap when nothing changed — Docker's layer cache skips every step
# whose inputs haven't changed, so this only actually rebuilds what you edited.
if [ "$ACTIVE_PROFILE" = "mtp" ]; then
  HOST_UID=$(id -u) HOST_GID=$(id -g "$(whoami)") docker compose --profile mtp up -d --build --remove-orphans --scale ollama=0
else
  docker stop llm-tulpa-llama-mtp-1 llm-tulpa-mtp-proxy-1 2>/dev/null || true
  HOST_UID=$(id -u) HOST_GID=$(id -g "$(whoami)") docker compose up -d --build --remove-orphans
fi

if [ "$BEFORE_URL" != "$AFTER_URL" ]; then
  docker compose restart backend
fi
