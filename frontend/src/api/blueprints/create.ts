import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Blueprint} from './types'

export const createBlueprint = async (name: string): Promise<Blueprint> => {
    const {data} = await axios.post<Blueprint>(`${BACKEND_URL}/api/blueprints`, {name})
    return data
};
