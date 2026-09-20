import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Soft-deletes a chat */
export const deleteChat = async (id: number): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/chats`, {params: {id}})
};
