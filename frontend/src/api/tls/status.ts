import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {TlsStatus} from './types'

/** Which HTTPS certificate the frontend container serves (owner only) */
export const getTlsStatus = async (): Promise<TlsStatus> => {
    const {data} = await axios.get<TlsStatus>(`${BACKEND_URL}/api/tls`)
    return data
};
