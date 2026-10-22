import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {MemoryResponse} from './types.ts'

/** Everything remembered about you, the most recently changed first */
export const getMemory = async (): Promise<MemoryResponse> => {
    const {data} = await axios.get<MemoryResponse>(`${BACKEND_URL}/api/memory`)
    return data
};
