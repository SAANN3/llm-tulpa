import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {MessagesResponse} from './types'

export interface GetMessagesQuery {
    chatId: number
    limit?: number
    skip?: number
}

/** Fetches a chat's messages, newest first */
export const getMessages = async (query: GetMessagesQuery): Promise<MessagesResponse> => {
    const {data} = await axios.get<MessagesResponse>(`${BACKEND_URL}/api/chats/messages`, {
        params: {chat_id: query.chatId, limit: query.limit, skip: query.skip},
    })
    return data
};
