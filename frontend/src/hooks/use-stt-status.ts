import {useCallback, useEffect, useState} from 'react'
import {getSttStatus} from '../api/stt/status.ts'
import type {SttStatus} from '../api/stt/types.ts'
import {debugLog} from '../utils/debug.ts'

/** How often the status is read while a model is being installed */
const POLL_MS = 1500

/**
 * What the backend has to say about voice input: the models, the install going on, whether the caller can use it at
 * all. Read when the page that asks opens, and again every moment while an install runs, so its progress and its end
 * show without a reload. `refresh` reads it now (after starting or removing an install).
 */
export const useSttStatus = (): { status: SttStatus | null; refresh: () => Promise<void> } => {
    const [status, setStatus] = useState<SttStatus | null>(null)

    const refresh = useCallback(async () => {
        try {
            setStatus(await getSttStatus())
        } catch (e) {
            // The button then stays as it is; the page says nothing of a status it could not read
            debugLog('voice', 'could not read the speech status', e)
        }
    }, [])

    useEffect(() => {
        void refresh()
    }, [refresh])

    const installing = status?.task?.state === 'running'
    useEffect(() => {
        if (!installing) return
        const id = setInterval(() => void refresh(), POLL_MS)
        return () => clearInterval(id)
    }, [installing, refresh])

    return {status, refresh}
};
