import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Drops one prompt waiting behind the chat's current turn. */
export const dropQueued = async (chatId: number, id: number): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/agent/queue`, {params: {chat_id: chatId, id}})
}
