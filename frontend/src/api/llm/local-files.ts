import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {LocalFiles} from './types'

/** The .gguf files in the backend's model directory (owner only) */
export const listLocalFiles = async (): Promise<LocalFiles> => {
    const {data} = await axios.get<LocalFiles>(`${BACKEND_URL}/api/llm/local_files`)
    return data
};
