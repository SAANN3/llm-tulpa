import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {HfFile} from './types'

/** The GGUF files of a repository */
export const listHfFiles = async (repo: string): Promise<HfFile[]> => {
    const {data} = await axios.get<{ files: HfFile[] }>(`${BACKEND_URL}/api/hf/files`, {params: {repo}})
    return data.files
};
