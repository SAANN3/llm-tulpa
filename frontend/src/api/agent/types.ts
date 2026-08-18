/**
 * Whether a tool call is permitted right now, given whatever scope's already been
 * granted for this chat. `escalation`, when present, is a broader scope the tool is
 * offering — pass its `scope` value back via `allowScope` (to grant it for the chat
 * going forward) or as `useTool`'s one-time `scope` override (to run just this once).
 * A `denied` with no `escalation` means the call can't be approved at all, one-time or
 * otherwise — still worth showing, just with nothing to offer the user to accept.
 */
export type AgentToolPermission =
  | { status: 'allowed' }
  | { status: 'denied'; reason: string; escalation: AgentScopeGrant | null }

export interface AgentScopeGrant {
  /** Opaque — hold onto it and pass it back verbatim, don't inspect its shape. */
  scope: unknown
  /** What granting this actually permits, in plain English. */
  ui_message: string
}

/** One tool call the model has requested, resolved or not. */
export interface AgentToolCall {
  permission: AgentToolPermission
  name: string
  arguments: Record<string, unknown>
}

/** Response shape shared by `chat` and `continue_chat` — the model's reply for a turn. */
export interface ChatOut {
  content: string
  created_at: string
  can_use_tools: boolean
  tool_calls: AgentToolCall[]
  /** The model's reasoning trace for this reply, when `think` was requested and the model produced one. */
  thinking: string | null
  /** How long the Ollama call for this reply took, in milliseconds. */
  thought_duration_ms: number
}

export interface CanUseTool {
  can_use: boolean
  tools: AgentToolCall[]
}

export interface UseToolOut {
  success: boolean
  /** Set when `success` is `false` specifically because the call wasn't permitted — distinct from a genuine tool failure. */
  denied: boolean
  tool_name: string
  err: string | null
  content: unknown
  created_at: string
  tools: AgentToolCall[]
}
