import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {CertInfo} from './types'

/** Replaces the HTTPS certificate with this one, PEM and without a password on the key (owner only) */
export const setCertificate = async (certificate: string, privateKey: string): Promise<CertInfo> => {
    const {data} = await axios.post<CertInfo>(`${BACKEND_URL}/api/tls/certificate`, {certificate, private_key: privateKey})
    return data
};
