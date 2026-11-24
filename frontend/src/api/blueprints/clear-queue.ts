import axios from 'axios'
import {BACKEND_URL} from '../../config'

export const clearBlueprintQueue = async (): Promise<number> => {
    const {data} = await axios.delete<{cancelled: number}>(`${BACKEND_URL}/api/blueprints/queue`)
    return data.cancelled
}
