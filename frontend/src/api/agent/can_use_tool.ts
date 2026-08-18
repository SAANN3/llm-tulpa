import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { CanUseTool } from './types'

/**
 * The tool calls the model has asked for in `chatId` that haven't been run yet, without
 * running them — for checking each one's `permission` before committing to `useTool`.
 * Mirrors `POST /api/agent/can_use_tool` on the backend.
 */
export async function canUseTool(chatId: number): Promise<CanUseTool> {
  const { data } = await axios.post<CanUseTool>(`${BACKEND_URL}/api/agent/can_use_tool`, {
    chat_id: chatId,
  })

  return data
}
