import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Marks a chat as seen: clears the note of how its last run ended */
export const setChatSeen = async (chatId: number): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/seen`, {chat_id: chatId})
};
