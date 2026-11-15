import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {BlueprintDocument} from './types'

export const getBlueprint = async (id: number, signal?: AbortSignal): Promise<BlueprintDocument> => {
    const {data} = await axios.get<BlueprintDocument>(`${BACKEND_URL}/api/blueprints/${id}`, {signal})
    return data
}
