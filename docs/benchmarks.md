# Why this model, and how it runs here

llm-tulpa runs one local model at a time — any tool-calling-capable model works, but it's worth knowing why this one specifically: it's a mixture-of-experts model, and despite its size, only a fraction of its weights are actually active per token. On a VRAM-constrained card that mattered more than raw parameter count — a same-generation *dense* model of comparable size, mostly spilled to system RAM, ran at a barely-usable ~1.5 tokens/sec on the 6GB card this project started on, while the MoE model held nearly the same speed as a much smaller dense model (~8 t/s there) while clearly outperforming it on real tasks (long single-shot generations, multi-step reasoning). If you're VRAM-constrained, it's worth looking for an MoE quant before ruling out anything bigger than what fits entirely in VRAM.

## This setup
- OS: Arch Linux
- ~~GPU: AMD RX 5600 XT — 6GB VRAM, gfx1010/RDNA1~~ *(old)*
- GPU: AMD RX 6800 — 16GB VRAM, gfx1030/RDNA2
- CPU: AMD Ryzen 5 3600 — 6 cores / 12 threads
- RAM: 39GB

Writing speed on this card, MTP on, 8-bit KV cache, flash attention, context sized automatically (mean of five runs over three code prompts — a long conversation is slower; ROCm on the host, Vulkan in the Docker image):

| Model | ROCm (native) | Vulkan (Docker image) |
|---|---|---|
| Qwen3.6-35B-A3B `UD-IQ3_XXS-MTP` | ~100 tok/s (context ~90k) | ~25 tok/s (~52k) |
| Qwen3.8-27B `UD-IQ3_S` | ~36 tok/s (~65k) | ~11.5 tok/s (~46k) |

The Vulkan column is the Docker image: most of the gap is the container, not Vulkan. Run on the host, the same Vulkan build was faster than ROCm on this card (2026-10-16, llama.cpp b11347, Qwen3.6-35B-A3B `UD-IQ3_S` fully on the GPU, no MTP, 8-bit KV cache, 98k context, one 6k-token and one 38k-token prompt):

| On the host | Writing (6k / 38k prompt) | Reading the prompt (6k / 38k) | VRAM left free | CPU busy |
|---|---|---|---|---|
| ROCm | ~55 / ~49 tok/s | ~820 / ~725 tok/s | ~0.7 GB | ~1.3 cores |
| Vulkan | ~70 / ~64 tok/s | ~690 / ~510 tok/s | ~1.1 GB | ~0.4 cores |
| Vulkan, `-ub 1024` | ~72 / ~64 tok/s | ~870 / ~600 tok/s | ~1.0 GB | ~0.4 cores |

Notes from the same tests:
- Keep about 1 GB of VRAM free. A profile that doesn't fit doesn't fail to load: it spills into system RAM, writes at 1–5 tok/s and makes the whole desktop stutter. With only 0.5–0.6 GB free, long prompts on Vulkan made the GPU time out and reset. Loading an image needs room for a moment too, so on this card the 35B-A3B reads images with the vision projector in RAM (about 7 s an image instead of 1–2).
- On this model the 4-bit KV cache cost very little: the same recall of 12 planted facts at 87k tokens as 8-bit (10 of 12 each), perplexity +0.25% against +0.08% for 8-bit, and the same speed on Vulkan, for twice the context (131k with 1.1 GB to spare).
- MTP speeds writing up by about 20% on ROCm but only about 4% on Vulkan, for 0.6–0.8 GB of VRAM.
- `IQ3_XXS`, `IQ3_S` and AesSedai's `IQ3_S` of Qwen3.6-35B-A3B came out the same against `UD-Q4_K_XL` (mean KL divergence 0.045–0.047).
- As a coding agent in this app, with one problem per chat and a way to check the result, Qwen3.6-35B-A3B fixed a bug from its symptom and built a small feature to a written spec in 2–7 minutes; Qwen3.8-27B did the same in about 12. A two-part phone CSS fix across a whole page set failed more often than it passed (Qwen3.6-35B-A3B passed it twice in six tries, across its quants), and its failed runs ended with it saying it was done while its own measuring tool said otherwise. gpt-oss-20b, Devstral Small 2, Qwen3-Coder-30B and GLM-4.7-Flash solved little of it.

ROCm needs the host's `hipblas` (the setup wizard says when it is missing) and the llama.cpp ROCm build; Vulkan works without extra libraries. On Nvidia the CUDA builds are offered the same way.
