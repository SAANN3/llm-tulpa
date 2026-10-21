import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Takes the uploaded certificate away, back to the one the container made (owner only) */
export const removeCertificate = async (): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/tls/certificate`)
};
