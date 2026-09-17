export type ThinkChoice = boolean | string

export type ThinkingCapability =
    | { kind: 'graduated'; modes: string[] }
    | { kind: 'on_off' }
    | { kind: 'unsupported' }

export type AgentToolPermission =
    | { status: 'allowed' }
    | { status: 'denied'; reason: string; escalation: AgentScopeGrant | null }

export interface AgentScopeGrant {
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
