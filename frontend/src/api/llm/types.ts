/** One entry of Ollama's public model library */
export interface CatalogModel {
    name: string
    description: string | null
    capabilities: string[]
    sizes: string[]
    pulls: string | null
}

export interface Catalog {
    /** `false` when ollama.com couldn't be reached and `models` is the built-in short list */
    live: boolean
    models: CatalogModel[]
}

export interface LocalFile {
    /** Relative to the model directory — what an import refers to it by */
    path: string
    size_bytes: number
    /** Read from the file's own header, not its name; `invalid` means it isn't a readable GGUF */
    kind: 'model' | 'projector' | 'invalid'
    error: string | null
    /** For a model: the projector files that fit it (all that can't be ruled out) */
    compatible_projectors: string[]
    /** For a model: the projector to preselect, when there's a clear answer */
    suggested_projector: string | null
}

export interface LocalFiles {
    /** Whether the backend has a model directory configured at all */
    configured: boolean
    files: LocalFile[]
}

export interface ModelTask {
    id: number
    kind: 'pull' | 'import'
    /** The model this produces */
    model: string
    state: 'running' | 'done' | 'failed'
    phase: string
    completed_bytes: number
    total_bytes: number
    error: string | null
}

export interface ImportItem {
    file: string
    name?: string
    projector?: string
}
