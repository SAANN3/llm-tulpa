#!/bin/sh
set -e
cd "$(dirname "$0")"
# --build is cheap when nothing changed — Docker's layer cache skips every step
# whose inputs haven't changed, so this only actually rebuilds what you edited.
HOST_UID=$(id -u) HOST_GID=$(id -g "$(whoami)") docker compose up -d --build
