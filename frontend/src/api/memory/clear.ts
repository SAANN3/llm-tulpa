import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Forgets everything remembered about you, your own entries included; how many entries went */
export const clearMemory = async (): Promise<number> => {
    const {data} = await axios.post<{ removed: number }>(`${BACKEND_URL}/api/memory/clear`)
    return data.removed
};
