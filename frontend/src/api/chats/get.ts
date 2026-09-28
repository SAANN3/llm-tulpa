import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {GetChatsResponse} from './types'

export interface GetChatsQuery {
    id?: number
    /** Scopes the list to one folder's chats. */
    folder_id?: number
    limit?: number
    skip?: number
}

/** Fetches one chat by id, or a page of chats when no id is given */
export const getChats = async (query: GetChatsQuery = {}): Promise<GetChatsResponse> => {
    const {data} = await axios.get<GetChatsResponse>(`${BACKEND_URL}/api/chats`, {
        params: query,
    })
    return data
};
