import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { ChatOut } from './types'

/**
 * Sends `prompt` as the next turn in `chatId`'s conversation and returns the model's
 * reply. If the reply carries tool calls, `can_use_tools` comes back `true` and the
 * caller drives `useTool`/`canUseTool` before asking for anything else. `think` asks
 * the model to reason before answering, and defaults to `true`. Mirrors
 * `POST /api/agent/chat` on the backend.
 */
export async function chat(chatId: number, prompt: string, think = true): Promise<ChatOut> {
  const { data } = await axios.post<ChatOut>(`${BACKEND_URL}/api/agent/chat`, {
    chat_id: chatId,
    prompt,
    think,
  })

  return data
}
