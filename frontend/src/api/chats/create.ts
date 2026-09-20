import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ChatOut} from './types'

/** Creates a new chat with the given name */
export const createChat = async (name: string): Promise<ChatOut> => {
    const {data} = await axios.post<ChatOut>(`${BACKEND_URL}/api/chats`, {name})
    return data
};
