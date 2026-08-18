import axios from 'axios'

import { BACKEND_URL } from '../../config'

/** Renames a chat. Mirrors `POST /api/chats/rename` on the backend. */
export async function renameChat(chatId: number, name: string): Promise<void> {
  await axios.post(`${BACKEND_URL}/api/chats/rename`, { chat_id: chatId, name })
}
