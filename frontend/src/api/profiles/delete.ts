import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Deletes a launch profile (owner only) */
export const deleteProfile = async (id: number): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/profiles`, {params: {id}})
};
