import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { ChatOut, ThinkChoice } from './types'

/**
 * Sends `prompt` as the next turn in `chatId`'s conversation and returns the model's
 * reply. If the reply carries tool calls, `can_use_tools` comes back `true` and the
 * caller drives `useTool`/`canUseTool` before asking for anything else. `think` asks
 * the model to reason before answering, and defaults to `true`. `images` (base64, no
 * data-URL prefix) requires a vision-capable model — see `llm/README.md`. `fileIds`
 * are ids of files already uploaded via `api/files`' `uploadFile` — only recorded on
 * the message for now, not yet fed to the model (see the backend's matching doc
 * comment on `Agent::chat`). Mirrors `POST /api/agent/chat` on the backend.
 */
export async function chat(
  chatId: number,
  prompt: string,
  think: ThinkChoice = true,
  images: string[] = [],
  fileIds: number[] = [],
): Promise<ChatOut> {
  const { data } = await axios.post<ChatOut>(`${BACKEND_URL}/api/agent/chat`, {
    chat_id: chatId,
    prompt,
    think,
    images,
    file_ids: fileIds,
  })

  return data
}
