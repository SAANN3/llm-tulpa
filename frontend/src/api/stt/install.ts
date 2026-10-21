import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SttTask} from './types'

/** Starts installing a speech model in the background (owner only); watch it with `getSttStatus` */
export const installSttModel = async (model: string): Promise<SttTask> => {
    const {data} = await axios.post<SttTask>(`${BACKEND_URL}/api/stt/models`, {model})
    return data
};
