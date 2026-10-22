import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {MemoryEntry, MemoryEntryRequest} from './types.ts'

/** Adds an entry you write yourself; the model never changes or drops it */
export const createMemory = async (entry: MemoryEntryRequest): Promise<MemoryEntry> => {
    const {data} = await axios.post<MemoryEntry>(`${BACKEND_URL}/api/memory`, entry)
    return data
};
