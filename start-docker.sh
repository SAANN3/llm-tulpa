#!/bin/sh
set -e
cd "$(dirname "$0")"
# The backend reads all of its settings from backend/data/settings.json (see its README). A
# fresh checkout doesn't have one yet — start from the Docker-ready example, which the backend
# then fills in (JWT secret, database) as the setup wizard runs.
if [ ! -f backend/data/settings.json ]; then
  cp backend/data/settings.docker.example.json backend/data/settings.json
fi
# --build is cheap when nothing changed — Docker's layer cache skips every step
# whose inputs haven't changed, so this only actually rebuilds what you edited.
HOST_UID=$(id -u) HOST_GID=$(id -g "$(whoami)") docker compose up -d --build
