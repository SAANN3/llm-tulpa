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
 * the tool offers: pass its `scope` back via `allowScope` (grant for the chat) or as
 * `useTool`'s one-time `scope` override (run just this once). A `denied` with no `escalation`
 * can't be approved at all.
 */
export type AgentToolPermission =
    | { status: 'allowed' }
    | { status: 'denied'; reason: string; escalation: AgentScopeGrant | null }

export interface AgentScopeGrant {
    /** Opaque: hold onto it and pass it back verbatim, don't inspect its shape. */
    scope: unknown
    ui_message: string
}

export interface AgentToolCall {
    permission: AgentToolPermission
    name: string
    arguments: Record<string, unknown>
}

export interface ChatOut {
    content: string
    created_at: string
    can_use_tools: boolean
    tool_calls: AgentToolCall[]
    thinking: string | null
    thought_duration_ms: number
    file_ids: number[]
}

export interface CanUseTool {
    can_use: boolean
    tools: AgentToolCall[]
}

export interface UseToolOut {
    success: boolean
    denied: boolean
    tool_name: string
    err: string | null
    content: unknown
    created_at: string
    tools: AgentToolCall[]
}
