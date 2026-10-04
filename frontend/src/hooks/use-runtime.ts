import {useCallback, useEffect, useState} from 'react'
import {getRuntime} from '../api/runtime/status'
import type {RuntimeStatus} from '../api/runtime/types'
import {useServerEvent} from './use-server-events.ts'

const POLL_MS = 1500

/**
 * What the model server is doing, kept fresh: every state change the backend announces refreshes it,
 * and while a model is loading it also polls, so a dropped event can't leave the page showing
 * "loading" for ever. `watching` polls regardless, for a page about to start something that loads a
 * model: its event connection may open after the backend announced the load, and an event that came
 * before it was listening is never repeated.
 */
export const useRuntime = (watching = false) => {
    const [status, setStatus] = useState<RuntimeStatus | null>(null)

    const refresh = useCallback(async () => {
        try {
            setStatus(await getRuntime())
        } catch {
            // The next event or tick retries; a page that can't reach the backend already says so.
        }
    }, [])

    useEffect(() => {
        void refresh()
    }, [refresh])

    useServerEvent('model_state', () => void refresh())

    const polling = watching || status?.state === 'starting'
    useEffect(() => {
        if (!polling) return
        void refresh()
        const id = setInterval(() => void refresh(), POLL_MS)
        return () => clearInterval(id)
    }, [polling, refresh])

    return {status, refresh}
};
