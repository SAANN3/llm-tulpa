import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {QueueState} from './types'

export const reorderBlueprintQueue = async (runIds: number[]): Promise<QueueState> => {
    const {data} = await axios.put<QueueState>(`${BACKEND_URL}/api/blueprints/queue`, {run_ids: runIds})
    return data
}
