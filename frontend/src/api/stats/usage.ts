import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {UsageStats, StatsRange} from './types'

/** The user's usage and speed per day over the last `days` days or `months` months (every day, quiet ones included) */
export const getUsageStats = async (range: StatsRange): Promise<UsageStats> => {
    const {data} = await axios.get<UsageStats>(`${BACKEND_URL}/api/stats/usage`, {params: range})
    return data
};
