import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {HfTask} from './types'

/** Starts downloading one file into the model folder (owner only) */
export const startHfDownload = async (repo: string, file: string): Promise<HfTask> => {
    const {data} = await axios.post<HfTask>(`${BACKEND_URL}/api/hf/download`, {repo, file})
    return data
};
