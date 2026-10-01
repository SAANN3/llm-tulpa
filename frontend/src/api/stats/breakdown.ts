import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Breakdown, StatsRange} from './types'

/** How the user's usage splits across models and tools over the last `days` days or `months` months */
export const getBreakdown = async (range: StatsRange): Promise<Breakdown> => {
    const {data} = await axios.get<Breakdown>(`${BACKEND_URL}/api/stats/breakdown`, {params: range})
    return data
};
