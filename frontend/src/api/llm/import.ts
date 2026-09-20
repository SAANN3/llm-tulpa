import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ImportItem, ModelTask} from './types'

/** Imports local .gguf files as models, one background task each (owner only) */
export const importFiles = async (imports: ImportItem[]): Promise<ModelTask[]> => {
    const {data} = await axios.post<ModelTask[]>(`${BACKEND_URL}/api/llm/import`, {imports})
    return data
};
