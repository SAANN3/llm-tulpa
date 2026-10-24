import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {InstallTask} from './types'

/** Starts installing a staged plugin; the install runs in the background and is followed with `getInstallTask` */
export const installStagedPlugin = async (id: string): Promise<InstallTask> => {
    const {data} = await axios.post<InstallTask>(`${BACKEND_URL}/api/plugins/external/install`, {id})
    return data
};
