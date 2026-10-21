import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Asks for a new self-signed certificate for this machine's addresses and the one in use (owner only) */
export const regenerateCertificate = async (): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/tls/regenerate`)
};
