import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Removes an installed speech model and its files (owner only) */
export const removeSttModel = async (model: string): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/stt/models`, {params: {model}})
};
