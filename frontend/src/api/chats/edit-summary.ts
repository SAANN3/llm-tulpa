import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Replaces the text of the chat's summary; refused (409) while the chat has a run, or before its first fold */
export const editChatSummary = async (chatId: number, summary: string): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/summary`, {chat_id: chatId, summary})
};
