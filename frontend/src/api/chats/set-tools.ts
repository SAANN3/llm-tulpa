import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Turns the model's tools on or off for one chat, from its next request on; refused (409) while the chat has a run */
export const setChatTools = async (chatId: number, enabled: boolean): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/tools`, {chat_id: chatId, enabled})
};
