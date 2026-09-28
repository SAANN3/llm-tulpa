import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {FolderOut} from './types'

/** Creates a new folder with the given name */
export const createFolder = async (name: string): Promise<FolderOut> => {
    const {data} = await axios.post<FolderOut>(`${BACKEND_URL}/api/folders`, {name})
    return data
};
