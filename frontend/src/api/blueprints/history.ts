import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RunHistory} from './types'

export const listBlueprintRuns = async (
    id: number,
    params: {offset: number; limit: number},
    signal?: AbortSignal,
): Promise<RunHistory> => {
    const {data} = await axios.get<RunHistory>(`${BACKEND_URL}/api/blueprints/${id}/runs`, {params, signal})
    return data
}
