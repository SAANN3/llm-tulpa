import { useContext } from 'react'

import { SettingsContext, type SettingsContextValue } from './SettingsContext'

/** The persisted user settings (or `null` if none exist yet) and a setter that persists new ones. Must be called under `SettingsProvider`. */
export function useSettings(): SettingsContextValue {
  const context = useContext(SettingsContext)

  if (!context) {
    throw new Error('useSettings must be used within a SettingsProvider')
  }

  return context
}
