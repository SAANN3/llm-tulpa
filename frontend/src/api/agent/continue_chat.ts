import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { ChatOut } from './types'

/**
 * Sends `chatId`'s existing history to the model as-is, with no new turn added, and
 * returns its reply — for getting the model's next response after `useTool` has
 * persisted a tool's result. `think` asks the model to reason before answering, and
 * defaults to `true`. Mirrors `POST /api/agent/continue` on the backend.
 */
export async function continueChat(chatId: number, think = true): Promise<ChatOut> {
  const { data } = await axios.post<ChatOut>(`${BACKEND_URL}/api/agent/continue`, {
    chat_id: chatId,
    think,
  })

  return data
}
