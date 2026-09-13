import { getFileDownloadUrl } from '../api/files/download'
import { getFile as getFileApi } from '../api/files/get'
import { listFiles as listFilesApi } from '../api/files/list'
import type { FileOut } from '../api/files/types'
import { uploadFile as uploadFileApi } from '../api/files/upload'

/**
 * Files for one chat — thin wrappers around the `files` API, nothing cached or
 * tracked locally in this hook: `list`/`get` always ask the backend fresh (`GET
 * /files?chat_id=`/`GET /files?id=`), so this can never drift from what's actually
 * stored there the way a locally-accumulated list could (e.g. a file uploaded from
 * another tab, or already attached to the chat from before this hook mounted).
 */
export function useFiles(chatId: number) {
  const upload = (file: File, readOnly?: boolean): Promise<FileOut> => uploadFileApi(file, chatId, readOnly)

  const list = (): Promise<FileOut[]> => listFilesApi(chatId)

  const get = (id: number): Promise<FileOut> => getFileApi(id)

  return { upload, list, get, getDownloadUrl: getFileDownloadUrl }
}
