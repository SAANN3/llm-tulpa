import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {FileOut} from './types'

/** Fetches a file's metadata by id */
export const getFile = async (id: number): Promise<FileOut> => {
    const {data} = await axios.get<FileOut>(`${BACKEND_URL}/api/files`, {params: {id}})
    return data
};
