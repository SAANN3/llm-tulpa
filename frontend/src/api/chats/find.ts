import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {FindChatsResponse} from './types'

export interface FindChatsQuery {
    query: string
    /** Also look in the chats' messages, not only their names (the backend's default is yes) */
    includeMessages?: boolean
    limit?: number
}

/** The user's chats whose name contains `query`, and with `includeMessages` those with a message
 * containing it (case-insensitive), most recently active first */
export const findChats = async (query: FindChatsQuery): Promise<FindChatsResponse> => {
    const {data} = await axios.get<FindChatsResponse>(`${BACKEND_URL}/api/chats/find`, {
        params: {
            query: query.query,
            include_messages: query.includeMessages,
            limit: query.limit,
        },
    })
    return data
};
