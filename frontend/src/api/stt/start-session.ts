import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SttSession} from './types'

/** Opens a live voice input on the caller's model and language */
export const startSttSession = async (): Promise<SttSession> => {
    const {data} = await axios.post<SttSession>(`${BACKEND_URL}/api/stt/sessions`)
    return data
};
