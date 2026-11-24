import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {QueueState} from './types'

export const getBlueprintQueue = async (signal?: AbortSignal): Promise<QueueState> => {
    const {data} = await axios.get<QueueState>(`${BACKEND_URL}/api/blueprints/queue`, {signal})
    return data
}
