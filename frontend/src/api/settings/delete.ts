import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Resets user settings, for testing the unconfigured-settings flow */
export const deleteSettings = async (): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/settings`)
};
