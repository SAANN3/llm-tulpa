import {getFileDownloadUrl} from '../api/files/download'
import {getFile as getFileApi} from '../api/files/get'
import {listFiles as listFilesApi} from '../api/files/list'
import type {FileOut} from '../api/files/types'
import {uploadFile as uploadFileApi} from '../api/files/upload'

/** File operations for one chat, always asking the backend fresh with nothing cached locally */
export const useFiles = (chatId: number) => {
    const upload = (file: File, readOnly?: boolean): Promise<FileOut> => uploadFileApi(file, chatId, readOnly)

    const list = (): Promise<FileOut[]> => listFilesApi(chatId)

    const get = (id: number): Promise<FileOut> => getFileApi(id)

    return {upload, list, get, getDownloadUrl: getFileDownloadUrl}
};
