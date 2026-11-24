import axios from 'axios'
import {BACKEND_URL} from '../../config'

export const cancelQueuedRun = async (runId: number): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/blueprints/queue/${runId}`)
}
