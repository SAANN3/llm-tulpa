import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Renames a folder */
export const renameFolder = async (folderId: number, name: string): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/folders/rename`, {folder_id: folderId, name})
};
