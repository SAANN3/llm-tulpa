import {useContext} from 'react'
import {SettingsContext, type SettingsContextValue} from './settings-context.ts'

/** The persisted user settings and a setter that persists new ones */
export const useSettings = (): SettingsContextValue => {
    const context = useContext(SettingsContext)

    if (!context) {
        throw new Error('useSettings must be used within a SettingsProvider')
    }

    return context
};
