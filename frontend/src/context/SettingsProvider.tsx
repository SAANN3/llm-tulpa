import { useEffect, useState, type ReactNode } from 'react'

import { SettingsContext } from './SettingsContext'
import { getSettings } from '../api/settings/get'
import { setSettings as setSettingsApi } from '../api/settings/set'
import type { Settings } from '../api/settings/types'

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettingsState] = useState<Settings | null>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    getSettings()
      .then(setSettingsState)
      .catch(() => setSettingsState(null))
      .finally(() => setLoading(false))
  }, [])

  const setSettings = async (next: Settings) => {
    await setSettingsApi(next)
    setSettingsState(next)
  }

  return (
    <SettingsContext.Provider value={{ settings, loading, setSettings }}>
      {children}
    </SettingsContext.Provider>
  )
}
