export interface HfRepo {
    id: string
    downloads: number
    likes: number
    /** The author makes people accept terms first; downloading needs a token in the settings */
    gated: boolean
}

export interface HfFile {
    /** Path inside the repository */
    path: string
    size_bytes: number
    /** A vision projector rather than a language model */
    projector: boolean
}

export interface HfTask {
    id: number
    repo: string
    file: string
    state: 'running' | 'done' | 'failed'
    phase: string
    completed_bytes: number
    total_bytes: number
    error: string | null
    /** Where the file lands, relative to the model folder */
    local_path: string
}
