/** Where the model went, as llama-server reported at startup */
export interface LoadFacts {
    layers_offloaded: number | null
    layers_total: number | null
    buffers: { name: string; mib: number }[]
    /** The context each slot got — with an automatic context, the server's own choice */
    context_per_slot: number | null
}

export type RuntimeState = 'stopped' | 'starting' | 'ready' | 'failed' | 'not_installed' | 'external'

export interface RuntimeStatus {
    state: RuntimeState
    profile_id: number | null
    model: string | null
    /** Why it failed, or what is missing */
    detail: string | null
    pid: number | null
    uptime_secs: number | null
    /** Who has a turn running on the loaded profile right now */
    holders: string[]
    /** How many requests are waiting for the model to be free of those turns */
    queued: number
    facts: LoadFacts
    binary: string | null
    version: string | null
}

export interface RuntimeLogs {
    first: number
    next: number
    lines: string[]
}

export interface SpeedTest {
    reply: string
    prompt_tokens: number | null
    generated_tokens: number | null
    prompt_tokens_per_second: number | null
    generated_tokens_per_second: number | null
    /** The share of drafted tokens the model accepted (0 to 1); null without MTP drafting */
    draft_acceptance: number | null
}

export interface ManagedModel {
    id: number
    /** The file, relative to the model directory */
    file: string
    display_name: string | null
    profile_ids: number[]
}
