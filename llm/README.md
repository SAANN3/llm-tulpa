# LLM
Ollama, configured to serve local models with tool-calling support.

## Setup
Copy [`../.env.example`](../.env.example) to `.env` in the repo root and edit it — every setting below reads from there once it exists (see [`compose.yaml`](./compose.yaml)). A `.env` in this folder still works for Ollama-only settings, but the root one is read by the backend's compose too.

## Getting models
Three ways, all usable together:
- **From the app** — the model picker pulls from Ollama's library or Hugging Face (`hf.co/<user>/<repo>`), and **imports any `.gguf` under `MODEL_DIR`** (several at once, optionally paired with an `mmproj` file for vision) without downloading anything. This needs the backend to see the same folder: put `MODEL_DIR` in the root `.env` (the root `compose.yaml` mounts it into the backend as `/models`, and this compose reads that file too) and set `model_dir` in the backend's `settings.json` — see [`../backend/README.md`](../backend/README.md).
- **`MODEL_FILE`** (optional) — a `.gguf` under `MODEL_DIR` that [`start.sh`](./start.sh) builds into an Ollama model named `local-llm` on container start, rebuilding it whenever `MODEL_FILE` or [`Modelfile.template`](./Modelfile.template) changes. Leave it empty to start Ollama with no model and add them from the app.
- **By hand** — `ollama create` / `ollama pull` against the running container.

`MODEL_DIR` is the folder your `.gguf` files live in — an absolute path, or one relative to this folder; it defaults to this folder. Point it at faster storage if that's where a large model should sit. `OLLAMA_DATA_DIR` (below) is where Ollama keeps its own copy of what it creates or pulls.

## Vision
Set `MMPROJ_FILE` to a mmproj/CLIP projector `.gguf` (same folder rules as `MODEL_FILE` — plain filename, read from `MODEL_DIR`) to pair it with `MODEL_FILE` and give the model vision. Leave it unset for a plain-text model. The projector has to actually match the base model's vision tower — mismatched pairs can build without error but produce garbage on real images, so verify with a real image before relying on it.

## Changing the context window
`OLLAMA_CONTEXT_LENGTH` sets how much context the model gets. Set it here **and** as `ollama.context_length` in the backend's `settings.json` (see [`../backend/README.md`](../backend/README.md)) — the backend derives its own token budgeting (the per-request `num_predict` cap, history-compaction thresholds) from that value, so the two need to agree.

## Getting a bigger context window on tight VRAM
`OLLAMA_KV_CACHE_TYPE=q8_0` (plus `OLLAMA_FLASH_ATTENTION=1`, required for the former to actually take effect at all — silently ignored otherwise) quantizes the KV cache itself, not the model weights, trading some precision for roughly half the VRAM cost per token of context — the difference between a context window that fits entirely on the GPU and one that spills part of the model to CPU (which costs far more speed than the quantization itself does; a partially-offloaded model was measured here at roughly a third the tokens/sec of the same model fully on GPU).

Deliberately not defaulted in `compose.yaml` — it's a quality-for-VRAM trade only worth making on hardware tight enough to need it, so it's opt-in via your own `.env`. To find the right context length once it's set: raise `OLLAMA_CONTEXT_LENGTH` and check the actual result rather than assuming — `docker logs <ollama container>` while it loads shows `load_tensors: offloaded N/66 layers to GPU` (or however many layers your model has); the number to chase is *all* of them. If it's short, step the context back down (and back up, later, is safe to try again after freeing VRAM elsewhere) until it says so.

## Other knobs
`OLLAMA_KEEP_ALIVE` — how long the model stays loaded in VRAM after the last request before Ollama unloads it.

`OLLAMA_LOAD_TIMEOUT` — how long Ollama waits for the model to finish loading before giving up. The default can be too short for a large, mostly-CPU-offloaded model.

`OLLAMA_DATA_DIR` — where Ollama's own model store (built from the `.gguf` via `ollama create`) lives, if not `./.ollama` alongside this file. Same reasoning as `MODEL_DIR` above: keeps it on faster storage if that's not where this project sits.
