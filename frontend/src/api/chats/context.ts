import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ChatContextResponse} from './types'

/** What fills a chat's context, part by part, and what it remembers past a fold */
export const getChatContext = async (chatId: number): Promise<ChatContextResponse> => {
    const {data} = await axios.get<ChatContextResponse>(`${BACKEND_URL}/api/chats/context`, {params: {chat_id: chatId}})
    return data
};
