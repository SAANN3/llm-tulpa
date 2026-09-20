import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ModelTask} from './types'

/** Running and recently finished pulls/imports (owner only) */
export const listTasks = async (): Promise<ModelTask[]> => {
    const {data} = await axios.get<ModelTask[]>(`${BACKEND_URL}/api/llm/tasks`)
    return data
};
