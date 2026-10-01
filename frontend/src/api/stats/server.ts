import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ServerStats} from './types'

/** What the model backend is running right now */
export const getServerStats = async (): Promise<ServerStats> => {
    const {data} = await axios.get<ServerStats>(`${BACKEND_URL}/api/stats/server`)
    return data
};
