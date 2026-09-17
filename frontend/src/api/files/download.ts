import { BACKEND_URL } from '../../config'

/** URL that serves a file's raw bytes for direct download or rendering */
export const getFileDownloadUrl = (id: number): string => `${BACKEND_URL}/api/files/download?id=${id}`;
