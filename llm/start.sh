#!/bin/sh
set -e
cd /root
: "${MODEL_FILE:?MODEL_FILE env var must be set (see compose.yaml)}"

sed "s|MODEL_FILE_PLACEHOLDER|${MODEL_FILE}|" Modelfile.template > Modelfile

# Optional vision support: pairs MODEL_FILE with a mmproj/CLIP projector .gguf.
# Inserted as a second FROM line right after the first — Ollama bundles a FROM'd
# projector as a separate layer and turns on the model's vision capability.
if [ -n "${MMPROJ_FILE:-}" ]; then
  sed -i "1a FROM models/${MMPROJ_FILE}" Modelfile
fi

ollama serve &
SERVE_PID=$!

until ollama list >/dev/null 2>&1; do
  sleep 1
done

MODEL_HASH="$(sha256sum Modelfile | cut -d' ' -f1)"

if ! ollama show local-llm >/dev/null 2>&1 || [ "$(cat .model_hash 2>/dev/null)" != "$MODEL_HASH" ]; then
  ollama create local-llm -f Modelfile
  echo "$MODEL_HASH" > .model_hash
fi

# Forces the model into memory now, so the (often multi-minute, on a large
# CPU-offloaded model) load cost happens once here at container start rather than
# silently landing on whichever request happens to be first — including one from a
# UserCacheService background loop tick that fires before the backend's even up.
# Uses the `ollama` CLI itself rather than curl: this image doesn't ship curl, and
# `ollama` is guaranteed present since the rest of this script already depends on
# it. Best-effort: if the call fails for any reason, skip it silently rather than
# aborting startup (`set -e`) — worst case is just the old lazy-load behavior on
# whatever request comes first.
echo "warming up local-llm..."
ollama run local-llm "Hi" >/dev/null 2>&1 \
  && echo "local-llm warmed up" \
  || echo "local-llm warm-up failed, will load lazily on first request instead"

wait "$SERVE_PID"
