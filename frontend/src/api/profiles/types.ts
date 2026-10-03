export interface LaunchProfile {
    id: number
    model_id: number
    /** The model's identity (a file name for llama.cpp) and its provider */
    model: string
    provider: string
    name: string
    mmproj_file: string | null
    /** Whether the projector runs on the GPU (true) or the CPU: less VRAM, slower image reading */
    mmproj_gpu: boolean
    /** null sizes the context to free memory */
    context_length: number | null
    cache_type_k: string
    cache_type_v: string
    flash_attn: boolean
    gpu_layers: number
    mtp: boolean
    spec_draft_n_max: number
    ngram_match: number
    ngram_min: number
    ngram_max: number
    extra_args: string
}

/** What a request sets; on update a left-out field keeps its value, except `mmproj_file` and `context_length`, replaced as sent */
export type LaunchProfileIn = Partial<Omit<LaunchProfile, 'id' | 'model_id' | 'model' | 'provider'>>
