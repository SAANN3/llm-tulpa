import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { GetChatsResponse } from './types'

export interface GetChatsQuery {
  id?: number
  limit?: number
  skip?: number
}

/**
 * `id` given → that chat's info (404 if it doesn't exist or is deleted). `id` omitted →
 * a page of non-deleted chats, newest-active first, plus a total count for pagination.
 * Mirrors `GET /api/chats` on the backend.
 */
export async function getChats(query: GetChatsQuery = {}): Promise<GetChatsResponse> {
  const { data } = await axios.get<GetChatsResponse>(`${BACKEND_URL}/api/chats`, {
    params: query,
  })

  return data
}
