import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { MessagesResponse } from './types'

export interface GetMessagesQuery {
  chatId: number
  limit?: number
  skip?: number
}

/**
 * A chat's messages, newest first — `skip` counts from the newest end, so `skip=0,
 * limit=50` gets the latest 50 and `skip=50, limit=50` gets the next 50 older ones.
 * Each message includes whatever tool calls it made, plus the total message count in
 * the chat for pagination. Mirrors `GET /api/chats/messages` on the backend.
 */
export async function getMessages(query: GetMessagesQuery): Promise<MessagesResponse> {
  const { data } = await axios.get<MessagesResponse>(`${BACKEND_URL}/api/chats/messages`, {
    params: { chat_id: query.chatId, limit: query.limit, skip: query.skip },
  })

  return data
}
