import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ValidationReport, Workflow} from './types'

export const validateBlueprint = async (workflow: Workflow, signal?: AbortSignal): Promise<ValidationReport> => {
    const {data} = await axios.post<ValidationReport>(`${BACKEND_URL}/api/blueprints/validate`, {workflow}, {signal})
    return data
}
