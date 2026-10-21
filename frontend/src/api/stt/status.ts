import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SttStatus} from './types'

/** Which speech models exist and are installed, how an install is going, and what the caller's voice input would use */
export const getSttStatus = async (): Promise<SttStatus> => {
    const {data} = await axios.get<SttStatus>(`${BACKEND_URL}/api/stt`)
    return data
};
