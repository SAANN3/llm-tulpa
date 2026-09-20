import axios from 'axios'
import {BACKEND_URL} from '../../config'

/**
 * URL that serves a file's raw bytes. The route needs the Bearer token, which a browser
 * doesn't attach to an `<img src>`, `<video src>` or `<a href>` — so never hand this straight to
 * one of those. Load it through `fetchFileBlob` (or `useFileBlobUrl` for a media element).
 */
export const getFileDownloadUrl = (id: number): string => `${BACKEND_URL}/api/files/download?id=${id}`;

/** Fetches a file's bytes through axios, which attaches the Bearer token */
export const fetchFileBlob = async (id: number): Promise<Blob> => {
    const {data} = await axios.get<Blob>(getFileDownloadUrl(id), {responseType: 'blob'})
    return data
};

/** Saves a file to disk under `fileName`, via a blob so the authenticated request is what downloads it */
export const saveFile = async (id: number, fileName: string): Promise<void> => {
    const url = URL.createObjectURL(await fetchFileBlob(id))
    const a = document.createElement('a')
    a.href = url
    a.download = fileName
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    URL.revokeObjectURL(url)
};
