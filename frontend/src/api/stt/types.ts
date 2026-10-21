/** One speech-to-text model that can be installed */
export interface SttModel {
    id: string
    label: string
    description: string
    /** The languages it understands, as a person reads them */
    languages: string
    download_bytes: number
    /** Roughly what it takes of memory while loaded */
    memory_bytes: number
    installed: boolean
    /** What its files take on disk, once installed */
    disk_bytes: number | null
    /** Loaded in memory now */
    loaded: boolean
}

export interface SttTask {
    model: string
    state: 'running' | 'done' | 'failed'
    phase: 'starting' | 'downloading' | 'checking' | 'unpacking' | 'installed'
    /** Bytes downloaded so far; while unpacking, how much of the archive has been read */
    completed_bytes: number
    total_bytes: number
    /** What is going wrong and being tried again, while it is */
    note: string | null
    error: string | null
}

/** What the owner may tune about how the models run */
export interface SttTuning {
    /** CPU threads a model reads with */
    threads: number
    /** Minutes of no use after which the loaded model is let go of; 0 keeps it for good */
    idle_unload_minutes: number
}

export interface SttBenchRun {
    threads: number
    /** How long the recording took to read */
    seconds: number
    /** How many times faster than real time */
    speed: number
}

export interface SttBenchModel {
    model: string
    label: string
    runs: SttBenchRun[]
    /** The fewest threads that come within a tenth of the fastest run */
    best_threads: number | null
}

/** A measurement of how fast the installed models read at each thread count */
export interface SttBenchmark {
    state: 'running' | 'done' | 'failed'
    audio_seconds: number
    done: number
    total: number
    /** What is being read now, while it is */
    note: string | null
    models: SttBenchModel[]
    error: string | null
}

export interface SttStatus {
    models: SttModel[]
    /** The install going on, or the last one that ended */
    task: SttTask | null
    /** Voice input can be used: some model is installed */
    ready: boolean
    /** The model the caller's voice input runs on; null when none is installed */
    model: string | null
    /** The language the caller chose for it; null means the model finds it out */
    language: string | null
    tuning: SttTuning
    /** How many cores this machine has */
    cores: number
    /** The measurement going on, or the last one that ended */
    benchmark: SttBenchmark | null
}

export interface SttSession {
    id: string
    model: string
}
