import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Owner-only: deletes a user and all their data */
export const deleteUser = async (id: number): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/users`, {params: {id}})
};
