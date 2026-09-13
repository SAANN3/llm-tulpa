import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { FileOut } from './types'

/**
 * Uploads `file` as a real multipart form (the same shape an HTML `<form
 * enctype="multipart/form-data">` posts), not JSON — the file's own name becomes the
 * stored, UI-facing `file_name`. `chatId` can be omitted to upload before a chat
 * exists yet (e.g. composing on the home page, before the first message that'll
 * create the chat has actually been sent) — the file comes back with `chat_id: null`
 * and gets claimed automatically the moment it's actually attached to a sent message
 * (see `chat` in `api/agent/chat.ts`). `readOnly` defaults to `true` (the backend's
 * own default) when omitted. Mirrors `POST /api/files/upload` on the backend.
 */
export async function uploadFile(file: File, chatId?: number, readOnly?: boolean): Promise<FileOut> {
  const form = new FormData()
  if (chatId !== undefined) form.append('chat_id', String(chatId))
  if (readOnly !== undefined) form.append('read_only', String(readOnly))
  form.append('file', file)

  const { data } = await axios.post<FileOut>(`${BACKEND_URL}/api/files/upload`, form)

  return data
}
