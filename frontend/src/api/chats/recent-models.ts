import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RecentModelsResponse} from './types'

/** The models the user's most recently active chats are set to, newest first, each once */
export const getRecentModels = async (limit = 3): Promise<RecentModelsResponse> => {
    const {data} = await axios.get<RecentModelsResponse>(`${BACKEND_URL}/api/chats/recent_models`, {params: {limit}})
    return data
};
