import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RuntimeStatus} from './types'

/** What the model server is doing right now */
export const getRuntime = async (): Promise<RuntimeStatus> => {
    const {data} = await axios.get<RuntimeStatus>(`${BACKEND_URL}/api/runtime`)
    return data
};
