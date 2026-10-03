import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {HfRepo} from './types'

/** Hugging Face repositories with GGUF files, most downloaded first */
export const searchHf = async (q: string): Promise<{ repos: HfRepo[]; has_token: boolean }> => {
    const {data} = await axios.get<{ repos: HfRepo[]; has_token: boolean }>(`${BACKEND_URL}/api/hf/search`, {params: {q}})
    return data
};
