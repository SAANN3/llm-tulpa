#!/bin/sh
set -e

# ROCm HIP backend discovery for AMD GPUs inside ollama:rocm image
export GGML_BACKEND_PATH=/usr/lib/ollama/rocm_v7_2/libggml-hip.so
export LD_LIBRARY_PATH=/usr/lib/ollama:/usr/lib/ollama/rocm_v7_2
export ROCR_VISIBLE_DEVICES=0

MODEL_PATH="/models/${MODEL_FILE:-Qwen3.6-35B-A3B-UD-IQ3_XXS-MTP.gguf}"
CTX="${OLLAMA_CONTEXT_LENGTH:-48000}"
KV_CACHE="${OLLAMA_KV_CACHE_TYPE:-q4_0}"
DRAFT_N="${LLAMA_MTP_SPEC_DRAFT_N_MAX:-3}"

ARGS="-m $MODEL_PATH --host 0.0.0.0 --port 8080 -ngl 99 --flash-attn on"
ARGS="$ARGS --cache-type-k $KV_CACHE --cache-type-v $KV_CACHE"
ARGS="$ARGS -c $CTX -n $CTX -np 1 --jinja --metrics"

if [ "$DRAFT_N" -gt 0 ] 2>/dev/null; then
  ARGS="$ARGS --spec-type draft-mtp --spec-draft-n-max $DRAFT_N"
fi

if [ -n "${MMPROJ_FILE:-}" ] && [ -f "/models/${MMPROJ_FILE}" ]; then
  ARGS="$ARGS --mmproj /models/${MMPROJ_FILE}"
fi

echo "=================================================="
echo "Starting llama-mtp server (ROCm GPU Acceleration):"
echo "Model:       $MODEL_PATH"
echo "Projector:   ${MMPROJ_FILE:-None (Text-only)}"
echo "Context:     $CTX"
echo "KV Cache:    $KV_CACHE"
echo "MTP Drafts:  $DRAFT_N"
echo "=================================================="

exec /usr/lib/ollama/llama-server $ARGS
