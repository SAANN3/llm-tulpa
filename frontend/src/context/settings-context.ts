import {createContext} from 'react'
import type {Settings, SettingsUpdate} from '../api/settings/types'

export interface SettingsContextValue {
    settings: Settings | null
    loading: boolean
    setSettings: (update: SettingsUpdate) => Promise<void>
}

export const SettingsContext = createContext<SettingsContextValue | null>(null)
