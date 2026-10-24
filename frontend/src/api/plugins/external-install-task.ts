import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {InstallTask} from './types'

/** How an install is going: what it is doing, and everything it has said so far */
export const getInstallTask = async (id: string): Promise<InstallTask> => {
    const {data} = await axios.get<InstallTask>(`${BACKEND_URL}/api/plugins/external/tasks`, {params: {id}})
    return data
};
