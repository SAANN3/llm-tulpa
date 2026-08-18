import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { GreetOut } from './greet'

/**
 * A very short (1-5 word) label summarizing `content`, written from the user's own
 * perspective — meant to be used as a chat/entry name. Generated live on each call, not
 * cached. Mirrors `POST /api/prompts/chat_name`.
 */
export async function chatName(content: string): Promise<GreetOut> {
  const { data } = await axios.post<GreetOut>(`${BACKEND_URL}/api/prompts/chat_name`, { content })

  return data
}
