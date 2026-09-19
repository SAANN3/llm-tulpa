import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SetupStatus} from './types'

/** Whether the backend has a database and an owner yet — drives first-run routing */
export const getSetupStatus = async (): Promise<SetupStatus> => {
    const {data} = await axios.get<SetupStatus>(`${BACKEND_URL}/api/setup/status`)
    return data
};
