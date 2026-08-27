# LLM
Ollama, configured to serve one local model with tool-calling support.

## Setting the model
Drop a `.gguf` file in this folder and point `MODEL_FILE` at its filename (see [`compose.yaml`](./compose.yaml)). [`start.sh`](./start.sh) builds an Ollama model from it named `local-llm` on first boot, and rebuilds it automatically whenever `MODEL_FILE` or [`Modelfile.template`](./Modelfile.template) changes — nothing to run by hand.

The `.gguf` doesn't have to live in this folder: set `MODEL_DIR` to point elsewhere (an absolute path, or a path relative to this folder), and that's where `MODEL_FILE` gets read from instead — useful for keeping a large model on faster storage than wherever this project itself happens to sit.

## Changing the context window
`OLLAMA_CONTEXT_LENGTH` sets how much context the model gets. Set it here **and** as the same-named env var on the backend (see [`../backend/README.md`](../backend/README.md)) — the backend derives its own token budgeting (the `num_predict` cap, history-compaction thresholds) from that value, so the two need to agree.

## Other knobs
`OLLAMA_KEEP_ALIVE` — how long the model stays loaded in VRAM after the last request before Ollama unloads it.
