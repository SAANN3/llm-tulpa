import { createContext } from 'react'

import type { ThemeName } from '../themes'

export interface ThemeContextValue {
  themeName: ThemeName
  setThemeName: (name: ThemeName) => void
  themeNames: readonly ThemeName[]
}

export const ThemeContext = createContext<ThemeContextValue | null>(null)
