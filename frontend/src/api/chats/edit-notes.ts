import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Replaces the model's notes in the chat (blank clears them); refused (409) while the chat has a run */
export const editChatNotes = async (chatId: number, notes: string): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/notes`, {chat_id: chatId, notes})
};
