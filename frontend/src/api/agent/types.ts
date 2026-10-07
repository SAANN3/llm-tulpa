/**
 * What to ask the model for on `think`: `false` disables reasoning, `true` enables it at the
 * model's own default effort, and a string requests a specific effort level (e.g. `"low"`) —
 * only meaningful when `ThinkingCapability` reports `kind: 'graduated'`.
 */
export type ThinkChoice = boolean | string

/**
 * What the active model supports for `think`, discovered from its own chat template on every
 * call (never cached). `graduated` lists the exact effort strings the template accepts;
 * `on_off` is a plain toggle; `unsupported` means the thinking control is hidden entirely.
 */

export type ThinkingCapability =
    | { kind: 'graduated'; modes: string[] }
    | { kind: 'on_off' }
    | { kind: 'unsupported' }

/**
 * Whether a tool call is permitted right now. `escalation`, when present, is a broader scope
 * the tool offers: the user's answer to it (`Allowance`) goes back with `answer`. A `denied` with
 * no `escalation` can't be approved at all.
 */
export type AgentToolPermission =
    | { status: 'allowed' }
    | { status: 'denied'; reason: string; escalation: AgentScopeGrant | null }

export interface AgentScopeGrant {
    /** Opaque: the backend keeps what it needs to apply the answer, the page only shows `ui_message`. */
    scope: unknown
    ui_message: string
}

export interface AgentToolCall {
    permission: AgentToolPermission
    name: string
    arguments: Record<string, unknown>
}

/** How the user answers a permission prompt for one pending tool call. */
export type Allowance = 'permanent' | 'only_now' | 'deny'

/** The answer to one pending tool call: `index` is its position in `TurnState.pending`. */
export interface Decision {
    index: number
    allowance: Allowance
}

export type RunEndReason = 'answered' | 'failed' | 'stopped' | 'step_limit' | 'waiting_for_permission'

/** How a run ended, kept until the next one starts. */
export interface RunEnded {
    reason: RunEndReason
    /** What failed, for `failed`. */
    detail: string | null
    ended_at: string
    started_at: string
    /** Tokens the run's model calls generated. */
    eval_tokens: number
    /** The HTTP status a failed run would have had; 423 means the model server is in use by someone else. */
    status: number | null
}

export type TurnStatus = 'idle' | 'running' | 'waiting_for_permission'

/** What a chat's turn is doing, for a page that was just opened or reloaded. */
export interface TurnState {
    status: TurnStatus
    started_at: string | null
    /** When the model call in flight started; null while a tool runs or between calls. */
    call_started_at: string | null
    eval_tokens: number
    prompt_tokens: number | null
    /** The step the run is on (1 for the first model call), and the user's step limit when one is set. */
    step: number
    step_limit: number | null
    running_tool: string | null
    tool_started_at: string | null
    /** While waiting for permission: the pending tool calls in order; `Decision.index` points into this list. */
    pending: AgentToolCall[]
    last_end: RunEnded | null
}

/** Returned by `startTurn`: the run goes on in the background. */
export interface StartTurnOut {
    /** The id the user's own message was stored under. */
    user_message_id: number
}

/** A chat with a run going on, from `GET /api/agent/runs` */
export interface RunningChat {
    chat_id: number
    /** The chat that started this one as a sub-agent, or null */
    parent_chat_id: number | null
    state: TurnState
}
