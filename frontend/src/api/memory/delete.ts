import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Forgets one entry; chats that already took it into their prompt keep it until their next fold */
export const deleteMemory = async (id: number): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/memory`, {params: {id}})
};
