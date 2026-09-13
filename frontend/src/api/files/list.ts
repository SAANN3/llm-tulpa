import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { FileOut } from './types'

interface FileListOut {
  files: FileOut[]
}

/** Every file recorded for a chat, in no particular order. Mirrors `GET /api/files?chat_id=` on the backend. */
export async function listFiles(chatId: number): Promise<FileOut[]> {
  const { data } = await axios.get<FileListOut>(`${BACKEND_URL}/api/files`, { params: { chat_id: chatId } })

  return data.files
}
