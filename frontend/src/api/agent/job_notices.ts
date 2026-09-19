import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { ChatOut, ThinkChoice } from './types'

/**
 * Reports background jobs that have finished to `chatId`'s model and returns its reply
 * (`notices` on it holds what was reported) — or `null`, without the model being called
 * at all, when no job has finished since it was last told about one. Safe to call on a
 * stale or duplicate `job_finished` event: whether there's anything to report is decided
 * on the backend. `think` defaults to `true`. Mirrors `POST /api/agent/job_notices`.
 */
export async function jobNotices(chatId: number, think: ThinkChoice = true): Promise<ChatOut | null> {
  const { data } = await axios.post<ChatOut | null>(`${BACKEND_URL}/api/agent/job_notices`, {
    chat_id: chatId,
    think,
  })

  return data
}
