import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { GreetOut } from './greet'

/** Generates a short chat name from a message's content and images */
export async function chatName(content: string, images: string[] = []): Promise<GreetOut> {
  const { data } = await axios.post<GreetOut>(`${BACKEND_URL}/api/prompts/chat_name`, { content, images })

  return data
}
