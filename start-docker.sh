#!/bin/sh
set -e
cd "$(dirname "$0")"

# The backend reads all of its settings from backend/data/settings.json (see its README). A
# fresh checkout doesn't have one yet — start from the Docker-ready example, which the backend
# then fills in (JWT secret, database) as the setup wizard runs.
if [ ! -f backend/data/settings.json ]; then
  cp backend/data/settings.docker.example.json backend/data/settings.json
fi

# A backend started by ./start-native.sh would clash with the container (same port, same bots): it has to
# be stopped first, by hand, since it is a process of yours in a terminal.
for pid in $(ss -ltnp 2>/dev/null | grep -E ':3000\b' | grep -o 'pid=[0-9]*' | cut -d= -f2 | sort -u); do
  case "$(readlink "/proc/$pid/exe" 2>/dev/null)" in
    "$PWD"/backend/target/*) echo "A native backend is running (pid $pid): stop it first (Ctrl-C in the terminal it runs in), then run this again." >&2; exit 1 ;;
  esac
done

# Which GPU the backend container gets, so the llama.cpp it runs can use it. Picked from the
# PCI vendor of the display devices the kernel lists (AMD 0x1002, NVIDIA 0x10de, anything else
# under /dev/dri), or forced with LLM_GPU=amd|nvidia|dri|none in the environment or the root
# .env. `none` runs on the CPU.
GPU="${LLM_GPU:-$(grep -E '^LLM_GPU=' .env 2>/dev/null | cut -d'=' -f2 | tr -d ' "')}"
if [ -z "$GPU" ]; then
  GPU=none
  for vendor_file in /sys/class/drm/card[0-9]*/device/vendor; do
    [ -f "$vendor_file" ] || continue
    case "$(cat "$vendor_file")" in
      0x1002) GPU=amd; break ;;
      0x10de) GPU=nvidia ;;
      *) [ "$GPU" = none ] && GPU=dri ;;
    esac
  done
fi

FILES="-f compose.yaml"
case "$GPU" in
  amd) FILES="$FILES -f backend/gpu-amd.yaml" ;;
  nvidia) FILES="$FILES -f backend/gpu-nvidia.yaml" ;;
  dri) FILES="$FILES -f backend/gpu-dri.yaml" ;;
  none) ;;
  *) echo "LLM_GPU must be amd, nvidia, dri or none (got '$GPU')" >&2; exit 1 ;;
esac
echo "GPU for the model server: $GPU"

# The device nodes belong to the host's video/render groups; the container needs those numeric ids.
VIDEO_GID=$(getent group video | cut -d: -f3)
RENDER_GID=$(getent group render | cut -d: -f3)
export VIDEO_GID="${VIDEO_GID:-44}" RENDER_GID="${RENDER_GID:-109}"

# --build is cheap when nothing changed — Docker's layer cache skips every step
# whose inputs haven't changed, so this only actually rebuilds what you edited.
# shellcheck disable=SC2086
HOST_UID=$(id -u) HOST_GID=$(id -g "$(whoami)") docker compose $FILES up -d --build --remove-orphans
