import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {NodeCatalog} from './types'

export const getBlueprintCatalog = async (signal?: AbortSignal): Promise<NodeCatalog> => {
    const {data} = await axios.get<NodeCatalog>(`${BACKEND_URL}/api/blueprints/catalog`, {signal})
    return data
}
