import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {HfTask} from './types'

/** The running and recently finished downloads (owner only) */
export const listHfTasks = async (): Promise<HfTask[]> => {
    const {data} = await axios.get<HfTask[]>(`${BACKEND_URL}/api/hf/tasks`)
    return data
};
