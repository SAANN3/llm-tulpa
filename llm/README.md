# LLM
Ollama, configured to serve one local model with tool-calling support.

## Setup
Copy [`.env.example`](./.env.example) to `.env` and edit it — every setting below reads from there once it exists (see [`compose.yaml`](./compose.yaml)).

## Setting the model
Drop a `.gguf` file in this folder and point `MODEL_FILE` at its filename (see [`compose.yaml`](./compose.yaml)). [`start.sh`](./start.sh) builds an Ollama model from it named `local-llm` on first boot, and rebuilds it automatically whenever `MODEL_FILE` or [`Modelfile.template`](./Modelfile.template) changes — nothing to run by hand.

The `.gguf` doesn't have to live in this folder: set `MODEL_DIR` to point elsewhere (an absolute path, or a path relative to this folder), and that's where `MODEL_FILE` gets read from instead — useful for keeping a large model on faster storage than wherever this project itself happens to sit.

## Vision
Set `MMPROJ_FILE` to a mmproj/CLIP projector `.gguf` (same folder rules as `MODEL_FILE` — plain filename, read from `MODEL_DIR`) to pair it with `MODEL_FILE` and give the model vision. Leave it unset for a plain-text model. The projector has to actually match the base model's vision tower — mismatched pairs can build without error but produce garbage on real images, so verify with a real image before relying on it.

## Changing the context window
`OLLAMA_CONTEXT_LENGTH` sets how much context the model gets. Set it here **and** as the same-named env var on the backend (see [`../backend/README.md`](../backend/README.md)) — the backend derives its own token budgeting (the `num_predict` cap, history-compaction thresholds) from that value, so the two need to agree.

## Other knobs
`OLLAMA_KEEP_ALIVE` — how long the model stays loaded in VRAM after the last request before Ollama unloads it.

`OLLAMA_LOAD_TIMEOUT` — how long Ollama waits for the model to finish loading before giving up. The default can be too short for a large, mostly-CPU-offloaded model.

`OLLAMA_DATA_DIR` — where Ollama's own model store (built from the `.gguf` via `ollama create`) lives, if not `./.ollama` alongside this file. Same reasoning as `MODEL_DIR` above: keeps it on faster storage if that's not where this project sits.
