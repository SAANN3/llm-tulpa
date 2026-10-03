import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RuntimeLogs} from './types'

/** The model server's latest output (owner only) */
export const getRuntimeLogs = async (limit = 300): Promise<RuntimeLogs> => {
    const {data} = await axios.get<RuntimeLogs>(`${BACKEND_URL}/api/runtime/logs`, {params: {limit}})
    return data
};
