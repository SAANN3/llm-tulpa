import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RunDetail} from './types'

export const getBlueprintRun = async (id: number, runId: number, signal?: AbortSignal): Promise<RunDetail> => {
    const {data} = await axios.get<RunDetail>(`${BACKEND_URL}/api/blueprints/${id}/runs/${runId}`, {signal})
    return data
}
