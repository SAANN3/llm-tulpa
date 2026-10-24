# Getting started in detail

The short version is in the [README](../README.md#quickstart); this is what sits behind it.

## A model
 Anything llama.cpp can run that supports tool calling works, but this project was built and tested against **[Qwen3.6-35B-A3B](https://huggingface.co/unsloth/Qwen3.6-35B-A3B-GGUF)**, specifically the `UD-Q4_K_XL` quant (`Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf`). Drop the `.gguf` in `llm/` (or a folder you point `MODEL_DIR` at), or search and download one from Hugging Face on the app's Models page. The app downloads and runs llama.cpp itself: the setup wizard looks at your hardware, installs the right build and has you pick the model file; every model setting (context, KV cache, GPU layers, MTP, vision projector, sampling) is editable in the app afterwards. Ollama still works as a second provider (`--profile ollama`; see [`llm/README.md`](../llm/README.md)).

## The setup wizard
 On first run, at `http://localhost:5173`, a setup wizard walks you through it — timezone, an **owner account** (username + password), the model server (it looks at your hardware, suggests the right llama.cpp build, downloads it — or uses a `llama-server` you point it at — and has you pick the model file), a theme, and notifications. With Postgres from the bundled compose file, the wizard's database step is one form (`localhost`, port `5432`, user/password `postgres`); the connection is saved to `backend/data/settings.json`, which also holds every other backend setting — see [`backend/README.md`](../backend/README.md). After setup you sign in with the owner account; the owner can add more users from the in-app Users page (there's no open self-registration), and each user's chats and settings are their own.

## Running the backend natively (for full speed)
The Docker image has the Vulkan build of llama.cpp, which works everywhere but runs several times slower inside the container than on the host. For the full speed run the backend on the host and keep the database, SearXNG and the frontend in Docker:
```bash
./start-native.sh
```
It starts those three in Docker (and stops the backend container), then runs `cargo run --release`, which rebuilds only what changed — the first build takes several minutes. Its settings live in `backend/data-native/settings.json`, apart from the Docker ones; a first run seeds that file from `backend/data/settings.json` (database block and secret included) with `model_dir` taken from `MODEL_DIR` in your `.env` (or choose the folder in the app), and `target/release/data` is a link to that folder, so a `cargo clean` doesn't take your settings with it. Open `http://localhost:5173`; in the setup wizard's llama.cpp step the suggested build is ROCm or CUDA when the host has its libraries (for AMD, `hipblas` — the wizard names what is missing). On the host the Vulkan build is worth trying too: on the RX 6800 below it wrote faster than ROCm (see [This setup](./benchmarks.md#this-setup)). Run only one backend at a time: `./start-docker.sh` refuses to start while a native one runs, and `./start-native.sh` stops the container.
