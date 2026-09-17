import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {FileOut} from './types'

/** Uploads a file as multipart form data */
export const uploadFile = async (file: File, chatId?: number, readOnly?: boolean): Promise<FileOut> => {
    const form = new FormData()
    if (chatId !== undefined) form.append('chat_id', String(chatId))
    if (readOnly !== undefined) form.append('read_only', String(readOnly))
    form.append('file', file)
    const {data} = await axios.post<FileOut>(`${BACKEND_URL}/api/files/upload`, form)
    return data
};
