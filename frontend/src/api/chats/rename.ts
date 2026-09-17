import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Renames a chat */
export const renameChat = async (chatId: number, name: string): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/rename`, {chat_id: chatId, name})
};
