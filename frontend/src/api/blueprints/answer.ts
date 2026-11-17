import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Allowance} from '../agent/types'

export const answerBlueprintRun = async (id: number, runId: number, requestId: string, allowance: Allowance): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/blueprints/${id}/runs/${runId}/answer`, {request_id: requestId, allowance})
}
