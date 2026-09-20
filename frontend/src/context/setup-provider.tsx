import {useCallback, useEffect, useRef, useState, type ReactNode} from 'react'
import {SetupContext} from './setup-context.ts'
import {setClientHandler} from '../api/client.ts'
import {getSetupStatus} from '../api/setup/status'
import type {SetupStatus} from '../api/setup/types'

export const SetupProvider = ({children}: { children: ReactNode }) => {
    const [status, setStatus] = useState<SetupStatus | null>(null)
    const [loading, setLoading] = useState(true)

    const refresh = useCallback(async () => {
        try {
            setStatus(await getSetupStatus())
        } catch {
            setStatus({configured: false, has_owner: false})
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

    return (
        <SetupContext.Provider value={{status, loading, refresh}}>
            {children}
        </SetupContext.Provider>
    )
};
