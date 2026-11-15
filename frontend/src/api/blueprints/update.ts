import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Blueprint} from './types'

export const updateBlueprint = async (blueprint: Blueprint, name: string): Promise<Blueprint> => {
    const {data} = await axios.put<Blueprint>(`${BACKEND_URL}/api/blueprints/${blueprint.id}`, {
        expected_revision: blueprint.revision,
        name,
        schedule: blueprint.schedule,
    })
    return data
};
