import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Literal} from './types'

export const getBlueprintVariables = async (id: number, signal: AbortSignal): Promise<Record<string, Literal>> => {
    const {data} = await axios.get<{values: Record<string, Literal>}>(`${BACKEND_URL}/api/blueprints/${id}/variables`, {signal})
    return data.values
}
