import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {UsageStats} from './types'

/** The user's token usage per UTC day over the last `days` days (only days with activity) */
export const getUsageStats = async (days: number): Promise<UsageStats> => {
    const {data} = await axios.get<UsageStats>(`${BACKEND_URL}/api/llm/stats`, {params: {days}})
    return data
};
