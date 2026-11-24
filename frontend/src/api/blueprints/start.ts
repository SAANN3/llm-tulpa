import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RunSummary} from './types'

export const startBlueprint = async (id: number, expectedRevision: number): Promise<RunSummary> => {
    const {data} = await axios.post<RunSummary>(`${BACKEND_URL}/api/blueprints/${id}/runs`, {expected_revision: expectedRevision})
    return data
}
