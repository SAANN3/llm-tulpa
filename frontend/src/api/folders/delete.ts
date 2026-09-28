import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Deletes a folder. Its chats aren't deleted — they're just ungrouped */
export const deleteFolder = async (id: number): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/folders`, {params: {id}})
};
