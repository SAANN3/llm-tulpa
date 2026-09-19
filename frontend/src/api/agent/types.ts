/** What to ask the model for on `think`: `false` disables reasoning, `true` enables
 * it at whatever the active model's own default effort is, and a string requests a
 * specific effort level (e.g. `"low"`) — only meaningful for a model whose
 * `GET /api/llm/thinking_capability` reports `kind: 'graduated'`; see
 * `ThinkingCapability` below and `UserInput`'s mode selector, which is the only
 * thing that ever produces a string value here. Mirrors the backend's own
 * `ThinkChoice` (`backend/src/services/llm.rs`) field-for-field. */
export type ThinkChoice = boolean | string

/** What the active model actually supports for `think` — `GET
 * /api/llm/thinking_capability`'s response shape, discovered fresh from the model's
 * own chat template on every call (nothing here is ever cached client-side either,
 * matching the backend's own no-persistence design — see that route's doc comment).
 * `graduated` lists the exact effort strings the model's template accepts, in the
 * order it lists them; `on_off` means only a plain toggle is supported (no levels to
 * offer); `unsupported` means hide any thinking control entirely for this model. */
export type ThinkingCapability =
  | { kind: 'graduated'; modes: string[] }
  | { kind: 'on_off' }
  | { kind: 'unsupported' }

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

/** Response shape shared by `chat`, `continue_chat` and `job_notices` — the model's reply for a turn. */
export interface ChatOut {
  content: string
  created_at: string
  can_use_tools: boolean
  tool_calls: AgentToolCall[]
  /** The model's reasoning trace for this reply, when `think` was requested and the model produced one. */
  thinking: string | null
  /** How long the Ollama call for this reply took, in milliseconds. */
  thought_duration_ms: number
  /** Ids of already-uploaded files attached to this reply, if any — a `ui.attach_file` call earlier in the same turn, resolved onto this (the turn's final, non-tool-calling) reply. */
  file_ids: number[]
  /** Background-job notices persisted just before this reply, oldest first — show them in the chat ahead of `content`, in this order. Empty unless a job finished since the previous turn. */
  notices: NoticeOut[]
}

/** One `notice` message: the backend telling the chat a background job ended. */
export interface NoticeOut {
  content: string
  created_at: string
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
