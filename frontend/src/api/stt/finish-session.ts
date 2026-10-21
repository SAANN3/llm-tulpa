import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Ends a live voice input: what is left is read and the whole text comes back */
export const finishSttSession = async (session: string): Promise<string> => {
    const {data} = await axios.post<{ text: string }>(`${BACKEND_URL}/api/stt/sessions/${session}/finish`)
    return data.text
};
