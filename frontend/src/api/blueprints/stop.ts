import axios from 'axios'
import {BACKEND_URL} from '../../config'

export const stopBlueprintRun = async (): Promise<number | null> => {
    const {data} = await axios.post<{run_id: number | null}>(`${BACKEND_URL}/api/blueprints/queue/stop`)
    return data.run_id
}
