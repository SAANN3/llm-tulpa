import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {BlueprintList, BlueprintSort} from './types'

export const listBlueprints = async (
    params: {offset: number; limit: number; q: string; sort: BlueprintSort},
    signal?: AbortSignal,
): Promise<BlueprintList> => {
    const {data} = await axios.get<BlueprintList>(`${BACKEND_URL}/api/blueprints`, {params, signal})
    return data
};
