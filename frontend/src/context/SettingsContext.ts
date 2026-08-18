import { createContext } from 'react'

import type { Settings } from '../api/settings/types'

export interface SettingsContextValue {
  settings: Settings | null
  /** True until the initial `GET /api/settings` on mount resolves. Lets a route guard hold off redirecting to `/setup` until it actually knows whether settings exist. */
  loading: boolean
  setSettings: (settings: Settings) => Promise<void>
}

export const SettingsContext = createContext<SettingsContextValue | null>(null)
