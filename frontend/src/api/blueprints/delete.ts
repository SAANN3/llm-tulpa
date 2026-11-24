import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Blueprint} from './types'

export const deleteBlueprint = async (blueprint: Blueprint): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/blueprints/${blueprint.id}`, {
        params: {expected_revision: blueprint.revision},
    })
};
