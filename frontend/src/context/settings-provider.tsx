import {useEffect, useState, type ReactNode} from 'react'
import {SettingsContext} from './settings-context.ts'
import {useAuth} from './use-auth.ts'
import {getSettings} from '../api/settings/get'
import {setSettings as setSettingsApi} from '../api/settings/set'
import type {Settings, SettingsUpdate} from '../api/settings/types'

export const SettingsProvider = ({children}: { children: ReactNode }) => {
    const {token} = useAuth()
    const [settings, setSettingsState] = useState<Settings | null>(null)
    const [loading, setLoading] = useState(true)

    useEffect(() => {
        if (!token) {
            setSettingsState(null)
            setLoading(false)
            return
        }

        let cancelled = false
        setLoading(true)
        getSettings()
            .then((s) => !cancelled && setSettingsState(s))
            .catch(() => !cancelled && setSettingsState(null))
            .finally(() => !cancelled && setLoading(false))

        return () => {
            cancelled = true
        }
    }, [token])

    const setSettings = async (update: SettingsUpdate) => {
        await setSettingsApi(update)
        setSettingsState((prev) => (prev ? {...prev, ...update} : prev))
    }

    return (
        <SettingsContext.Provider value={{settings, loading, setSettings}}>
            {children}
        </SettingsContext.Provider>
    )
};
