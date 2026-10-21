import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SttTuning} from './types'

/** Sets how many threads the models read with and how long an idle one stays loaded (owner only); applies at once */
export const saveSttTuning = async (tuning: SttTuning): Promise<SttTuning> => {
    const {data} = await axios.post<SttTuning>(`${BACKEND_URL}/api/stt/tuning`, tuning)
    return data
};
