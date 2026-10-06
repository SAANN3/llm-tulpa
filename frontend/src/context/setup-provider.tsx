import axios from 'axios'
import {useCallback, useEffect, useRef, useState, type ReactNode} from 'react'
import {SetupContext} from './setup-context.ts'
import {setClientHandler} from '../api/client.ts'
import {BACKEND_URL} from '../config'
import {getSetupStatus} from '../api/setup/status'
import type {SetupStatus} from '../api/setup/types'

export const SetupProvider = ({children}: { children: ReactNode }) => {
    const [status, setStatus] = useState<SetupStatus | null>(null)
    const [loading, setLoading] = useState(true)
    const [unreachable, setUnreachable] = useState<string | null>(null)

    const refresh = useCallback(async () => {
        try {
            setStatus(await getSetupStatus())
            setUnreachable(null)
        } catch (e) {
            // A backend that can't be asked is not one without a database: sending the user to the setup wizard
            // for that would invite them to configure a backend that is merely down. The last known status stays.
            setUnreachable(
                axios.isAxiosError(e) && e.response
                    ? `${BACKEND_URL} answered ${e.response.status} to the status request.`
                    : `${BACKEND_URL} did not answer${axios.isAxiosError(e) && e.message ? ` (${e.message})` : ''}.`,
            )
        }
    }, [])

    useEffect(() => {
        refresh().finally(() => setLoading(false))
    }, [refresh])

    // A 503 means the backend has no working database: re-reading the status flips it to
    // `configured: false`, and the route guard sends the user to /setup. One refresh at a time,
    // since a page usually has several requests in flight when that happens.
    const refreshing = useRef(false)
    useEffect(() => setClientHandler('onSetupRequired', () => {
        if (refreshing.current) return
        refreshing.current = true
        refresh().finally(() => {
            refreshing.current = false
        })
    }), [refresh])

    // A request that got no answer at all: asked again with the one the page can't do without, which says whether the backend is down
    useEffect(() => setClientHandler('onUnreachable', () => {
        if (refreshing.current) return
        refreshing.current = true
        refresh().finally(() => {
            refreshing.current = false
        })
    }), [refresh])

    return (
        <SetupContext.Provider value={{status, loading, unreachable, refresh}}>
            {children}
        </SetupContext.Provider>
    )
};
