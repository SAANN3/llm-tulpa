import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {MemoryEntry, UpdateMemoryRequest} from './types.ts'

/** Changes an entry's category and text; it is yours from then on */
export const updateMemory = async (entry: UpdateMemoryRequest): Promise<MemoryEntry> => {
    const {data} = await axios.post<MemoryEntry>(`${BACKEND_URL}/api/memory/update`, entry)
    return data
};
