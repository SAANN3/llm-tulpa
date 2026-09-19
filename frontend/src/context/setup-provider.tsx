import {useCallback, useEffect, useState, type ReactNode} from 'react'
import {SetupContext} from './setup-context.ts'
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

    return (
        <SetupContext.Provider value={{status, loading, refresh}}>
            {children}
        </SetupContext.Provider>
    )
};
