import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ExternalPlugin} from './types'

/** Every external plugin the backend has seen, running or not, with the reason for one that is not */
export const getExternalPlugins = async (): Promise<ExternalPlugin[]> => {
    const {data} = await axios.get<ExternalPlugin[]>(`${BACKEND_URL}/api/plugins/external`)
    return data
};
