import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {MessageSearchResponse} from './types'

export interface SearchMessagesQuery {
    chatId: number
    query: string
    limit?: number
    includeAssistant?: boolean
    includeUser?: boolean
    includeThinking?: boolean
    includeTools?: boolean
}

/** Finds the messages of a chat whose content contains `query` (case-insensitive), newest first */
export const searchMessages = async (query: SearchMessagesQuery): Promise<MessageSearchResponse> => {
    const {data} = await axios.get<MessageSearchResponse>(`${BACKEND_URL}/api/chats/search`, {
        params: {
            chat_id: query.chatId,
            query: query.query,
            limit: query.limit,
            include_assistant: query.includeAssistant,
            include_user: query.includeUser,
            include_thinking: query.includeThinking,
            include_tools: query.includeTools,
        },
    })
    return data
};
