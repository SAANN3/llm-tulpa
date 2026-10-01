import {useEffect, useState} from 'react'
import {getContextStats} from '../api/stats/context'
import {getServerStats} from '../api/stats/server'
import type {ContextStats, ServerStats} from '../api/stats/types'

const POLL_MS = 5000

/** What the model backend is running and how full the user's chats are, refreshed while mounted.
 * A failed refresh keeps the last good answer on screen and tries again at the next tick. */
export const useServerStats = () => {
    const [server, setServer] = useState<ServerStats | null>(null)
    const [context, setContext] = useState<ContextStats | null>(null)

    useEffect(() => {
        let cancelled = false
        const refresh = () => {
            getServerStats().then((next) => !cancelled && setServer(next), () => {})
            getContextStats().then((next) => !cancelled && setContext(next), () => {})
        }
        refresh()
        const timer = window.setInterval(refresh, POLL_MS)
        return () => {
            cancelled = true
            window.clearInterval(timer)
        }
    }, [])

    return {server, context}
};
