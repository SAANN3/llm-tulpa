import axios from 'axios'

import { BACKEND_URL } from '../../config'

/**
 * Soft-deletes a chat — it stops showing up in `getChats`, but its row and messages
 * stay in the database. Mirrors `DELETE /api/chats` on the backend.
 */
export async function deleteChat(id: number): Promise<void> {
  await axios.delete(`${BACKEND_URL}/api/chats`, { params: { id } })
}
