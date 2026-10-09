import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Replaces the chat's key facts (the goal stays); refused (409) while the chat has a run, or before its first fold */
export const editChatFacts = async (chatId: number, facts: string[]): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/facts`, {chat_id: chatId, facts})
};
