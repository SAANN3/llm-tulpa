#!/bin/sh
# Runs the backend on the host instead of in Docker — the way to get llama.cpp's ROCm (AMD) or CUDA
# (Nvidia) build, which the Docker image can't run — and keeps the database, SearXNG and the
# frontend in Docker. `./start-docker.sh` is the all-in-Docker alternative; run only one of them,
# since two backends would poll the same messaging bots.
#
# The native backend's settings live in backend/data-native/settings.json, apart from the Docker
# one (Docker's has container paths such as /models and /hostfs), and target/release/data is a link
# to that folder: the backend reads <exe_dir>/data/settings.json, and a `cargo clean` must not take
# the database connection and JWT secret with it.
#
# START_NATIVE_DRY_RUN=1 only prepares the settings and the link.
set -e
cd "$(dirname "$0")"

DATA=backend/data-native
LINK=backend/target/release/data

mkdir -p "$DATA" backend/target/release

# A first run seeds the native settings from the Docker ones (the database block, the JWT secret and
# the rest), with the paths that only mean something inside a container changed.
if [ ! -f "$DATA/settings.json" ]; then
  if [ -d "$LINK" ] && [ ! -L "$LINK" ] && [ -f "$LINK/settings.json" ]; then
    # Settings someone placed by hand next to the binary: keep them
    cp "$LINK/settings.json" "$DATA/settings.json"
  elif [ -f backend/data/settings.json ] && command -v python3 >/dev/null 2>&1; then
    MODEL_DIR_VALUE=$(grep -E '^MODEL_DIR=' .env 2>/dev/null | cut -d'=' -f2- | tr -d ' "')
    MODEL_DIR_VALUE="$MODEL_DIR_VALUE" python3 - <<'PY'
import json, os
settings = json.load(open("backend/data/settings.json"))
models = os.environ.get("MODEL_DIR_VALUE", "")
settings["model_dir"] = models if models.startswith("/") else None
settings["host_root"] = None
json.dump(settings, open("backend/data-native/settings.json", "w"), indent=2)
PY
  else
    cp backend/data/settings.example.json "$DATA/settings.json"
  fi
  echo "Native settings: $DATA/settings.json (edit model_dir there, or choose the folder in the app)"
fi

# The link goes in place of a plain data folder
if [ -d "$LINK" ] && [ ! -L "$LINK" ]; then
  mv "$LINK" "$LINK.old-$(date +%s)"
fi
ln -sfn ../../data-native "$LINK"

[ -n "$START_NATIVE_DRY_RUN" ] && exit 0

command -v cargo >/dev/null 2>&1 || { echo "cargo isn't installed: https://rustup.rs" >&2; exit 1; }

export HOST_UID="$(id -u)" HOST_GID="$(id -g "$(whoami)")"
# The https certificate of the frontend container (voice input needs a secure page): its folder, which the backend
# also reads and writes (its `tls_dir` defaults to the same place), made here so Docker doesn't create it as root, and
# the names the self-signed certificate is made for: this machine's, so the address you open it by is on it.
TLS_DIR="${TLS_DIR:-$HOME/.llm-tulpa/tls}"
mkdir -p "$TLS_DIR"
export TLS_DIR
ADDRESSES="$(ip -o -4 addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | tr '\n' ',')"
[ -n "$ADDRESSES" ] || ADDRESSES="$(hostname -I 2>/dev/null | tr ' ' ',')"
TLS_NAMES="localhost,127.0.0.1,$(hostname 2>/dev/null),$ADDRESSES"
export TLS_NAMES="$(echo "$TLS_NAMES" | tr -s ',' | sed 's/^,//;s/,$//')"
docker compose stop backend 2>/dev/null || true
docker compose up -d postgres searxng searxng-nginx
# --no-deps: the frontend would otherwise start the backend container too
docker compose up -d --build --no-deps frontend

cd backend
exec cargo run --release
