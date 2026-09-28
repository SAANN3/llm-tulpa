import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {GetFoldersResponse} from './types'

export interface GetFoldersQuery {
    id?: number
    /** Case-insensitive substring filter on the folder name */
    q?: string
    limit?: number
    skip?: number
}

/** Fetches one folder by id, or a page of folders (ordered by their most recently active
 * chat) when no id is given */
export const getFolders = async (query: GetFoldersQuery = {}): Promise<GetFoldersResponse> => {
    const {data} = await axios.get<GetFoldersResponse>(`${BACKEND_URL}/api/folders`, {
        params: query,
    })
    return data
};
