import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RuntimeStatus} from './types'

/** Stops the model server (owner only) */
export const stopRuntime = async (): Promise<RuntimeStatus> => {
    const {data} = await axios.post<RuntimeStatus>(`${BACKEND_URL}/api/runtime/stop`)
    return data
};
