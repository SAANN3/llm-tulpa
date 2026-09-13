import { BACKEND_URL } from '../../config'

/**
 * The URL a file's raw bytes are served from — navigating to it (an `<a href>`, an
 * `<img src>`, `window.open`, ...) downloads/renders the file directly, with
 * `Content-Disposition: attachment` and its UI-facing name set by the backend. Not an
 * API call itself (nothing here fetches anything): the browser is meant to load this
 * URL on its own. Mirrors `GET /api/files/download` on the backend.
 */
export function getFileDownloadUrl(id: number): string {
  return `${BACKEND_URL}/api/files/download?id=${id}`
}
