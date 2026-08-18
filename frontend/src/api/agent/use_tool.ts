import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { UseToolOut } from './types'

/**
 * Runs the next pending tool call for `chatId`, in the order the model requested them,
 * and persists its result — one call per request. `tools` in the response lists what's
 * still left; loop this until it's empty, then call `continueChat`. `scope`, if given,
 * overrides whatever's already been granted for this call only (never persisted) —
 * pass it when the user has just approved a one-time use of an `escalation` offered by
 * `AgentToolCall.permission`. Omit it to run against whatever's already stored, if
 * anything. Mirrors `POST /api/agent/use_tool` on the backend.
 */
export async function useTool(chatId: number, scope?: unknown): Promise<UseToolOut> {
  const { data } = await axios.post<UseToolOut>(`${BACKEND_URL}/api/agent/use_tool`, {
    chat_id: chatId,
    scope: scope ?? null,
  })

  return data
}
