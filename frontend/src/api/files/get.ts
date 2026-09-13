import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { FileOut } from './types'

/** A file's metadata by id. Mirrors `GET /api/files` on the backend. */
export async function getFile(id: number): Promise<FileOut> {
  const { data } = await axios.get<FileOut>(`${BACKEND_URL}/api/files`, { params: { id } })

  return data
}
