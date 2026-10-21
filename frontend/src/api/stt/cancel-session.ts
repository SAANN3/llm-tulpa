import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Drops a live voice input without reading the rest */
export const cancelSttSession = async (session: string): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/stt/sessions/${session}`)
};
