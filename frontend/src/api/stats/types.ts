/** The span a stats request covers: the last `days` days, or the last `months` calendar months
 * (from the 1st of the earliest to today) */
export type StatsRange = { days: number } | { months: number }

/** One day of a user's replies and what they cost, in the user's own timezone */
export interface UsageDay {
    /** `YYYY-MM-DD` */
    day: string
    replies: number
    /** Summed over the replies — the whole context is re-sent with every call */
    prompt_tokens: number
    eval_tokens: number
    /** Generation speed is `timed_eval_tokens / eval_ms`, over the replies that carry a timing */
    timed_replies: number
    timed_eval_tokens: number
    eval_ms: number
    /** Prompt-processing speed is `processed_tokens / processed_ms`, over the calls whose backend
     * reports how many prompt tokens it actually evaluated */
    processed_calls: number
    processed_tokens: number
    processed_ms: number
    load_ms: number
    /** Whole-call length, request to response; null on a day without calls */
    call_ms_median: number | null
    call_ms_p95: number | null
    /** Calls of 30 seconds or more — in practice a prompt the server had to evaluate again */
    slow_calls: number
}

export interface UsageStats {
    /** Every day of the range, oldest first, quiet days included */
    days: UsageDay[]
}

export interface ModelUsage {
    provider: string
    model: string
    chats: number
    replies: number
    eval_tokens: number
    timed_eval_tokens: number
    eval_ms: number
    call_ms_median: number | null
}

export interface ToolUsage {
    tool: string
    calls: number
    /** Ran and failed */
    failed: number
    /** Never ran: the permission system refused the call */
    denied: number
}

export interface Breakdown {
    models: ModelUsage[]
    tools: ToolUsage[]
}

export interface JobKindUsage {
    /** `process` (a shell command) or `agent` (a sub-agent) */
    kind: string
    total: number
    succeeded: number
    failed: number
    killed: number
    lost: number
    average_seconds: number | null
}

/** One day of the user's own messages, per hour */
export interface ActivityDay {
    /** `YYYY-MM-DD` */
    day: string
    /** 24 entries, midnight first */
    hours: number[]
}

export interface Activity {
    /** Every day of the range, oldest first */
    days: ActivityDay[]
    chats_per_day: { day: string; chats: number }[]
    jobs: JobKindUsage[]
}

export interface ContextStats {
    context_length: number
    largest: { chat_id: number; name: string; prompt_tokens: number }[]
    compacted_chats: number
    total_chats: number
}

export interface RunningModel {
    name: string
    size: number | null
    size_vram: number | null
    /** RFC 3339 */
    expires_at: string | null
    context_length: number | null
}

/** What llama-server reports about itself — only the `mtp` backend does */
export interface LlamaServer {
    model_file: string | null
    slots_total: number | null
    slots_processing: number | null
    prompt_tokens_total: number | null
    prompt_tokens_cached_total: number | null
    prompt_tokens_per_second: number | null
    predicted_tokens_total: number | null
    predicted_tokens_per_second: number | null
    draft_tokens_total: number | null
    draft_tokens_accepted_total: number | null
}

export interface ServerStats {
    reachable: boolean
    models: RunningModel[]
    llama_server: LlamaServer | null
}
