import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ContextStats} from './types'

/** How full the user's biggest chats are against the context window */
export const getContextStats = async (): Promise<ContextStats> => {
    const {data} = await axios.get<ContextStats>(`${BACKEND_URL}/api/stats/context`)
    return data
};
