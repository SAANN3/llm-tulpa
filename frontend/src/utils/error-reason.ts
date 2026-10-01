import axios from 'axios'

/** The backend's own explanation when it gave one (`{error}`), else a generic fallback */
export const errorReason = (e: unknown, fallback: string): string =>
    (axios.isAxiosError(e) && (e.response?.data as { error?: string } | undefined)?.error) || fallback
