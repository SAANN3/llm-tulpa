// Parts of a file name that describe how the weights are stored, not which model it is
const STORAGE_PART = /^(UD|MTP|I?Q\d.*|F16|BF16|F32|K_[SML]|imat|imatrix|GGUF)$/i
const SIZE_PART = /^\d+(\.\d+)?[BM]$/i

/** A model file name cut to what tells models apart: up to its size (`27B`) plus one more part that isn't a
 * quantization tag, so `Qwen3.6-35B-A3B-UD-IQ3_XXS-MTP.gguf` reads `Qwen3.6-35B-A3B`. A name without a size is
 * cut at its first quantization tag. Ollama names (`qwen3:8b`) are short already and stay as they are. */
export const shortModelName = (name: string): string => {
    if (name.includes(':')) return name
    const parts = name.replace(/\.gguf$/i, '').split('-')
    const size = parts.findIndex((part) => SIZE_PART.test(part))
    const kept: string[] = []
    for (const [i, part] of parts.entries()) {
        if (STORAGE_PART.test(part)) break
        kept.push(part)
        if (size >= 0 && i > size) break
    }
    return kept.length > 0 ? kept.join('-') : name
};
