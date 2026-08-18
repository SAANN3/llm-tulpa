import axios from 'axios'

import { BACKEND_URL } from '../../config'

/**
 * Persists a scope grant for `toolName` within `chatId`, so future calls to that tool
 * in that chat can run without asking again. `scope` should be whatever came back in
 * an `AgentToolCall.permission`'s `escalation.scope` (from `chat`/`can_use_tool`) — it's
 * opaque, just pass it back verbatim. Doesn't run anything itself; call `useTool`
 * separately afterward. Mirrors `POST /api/agent/allow_scope` on the backend.
 */
export async function allowScope(chatId: number, toolName: string, scope: unknown): Promise<void> {
  await axios.post(`${BACKEND_URL}/api/agent/allow_scope`, {
    chat_id: chatId,
    tool_name: toolName,
    scope,
  })
}
