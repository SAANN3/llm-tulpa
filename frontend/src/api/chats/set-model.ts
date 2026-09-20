import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Rebinds a chat to another model, effective from its next turn */
export const setChatModel = async (chatId: number, model: string, provider = 'ollama'): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/model`, {chat_id: chatId, model, provider})
};
