import {createContext} from 'react'
import type {Settings} from '../api/settings/types'

export interface SettingsContextValue {
    settings: Settings | null
    loading: boolean
    setSettings: (settings: Settings) => Promise<void>
}

export const SettingsContext = createContext<SettingsContextValue | null>(null)
