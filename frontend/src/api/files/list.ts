import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {FileOut} from './types'

interface FileListOut {
    files: FileOut[]
}

/** Fetches every file recorded for a chat */
export const listFiles = async (chatId: number): Promise<FileOut[]> => {
    const {data} = await axios.get<FileListOut>(`${BACKEND_URL}/api/files`, {params: {chat_id: chatId}})
    return data.files
};
