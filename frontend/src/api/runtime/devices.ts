import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** What `llama-server --list-devices` printed, errors included */
export const getDevices = async (): Promise<string> => {
    const {data} = await axios.get<{ output: string }>(`${BACKEND_URL}/api/runtime/devices`)
    return data.output
};
