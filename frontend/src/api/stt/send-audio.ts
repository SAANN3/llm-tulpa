import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Adds a chunk of audio (16-bit little-endian mono at 16 kHz) to a live voice input; the answer is the text so far */
export const sendSttAudio = async (session: string, audio: ArrayBuffer): Promise<string> => {
    const {data} = await axios.post<{ text: string }>(`${BACKEND_URL}/api/stt/sessions/${session}/audio`, audio, {
        headers: {'Content-Type': 'application/octet-stream'},
    })
    return data.text
};
