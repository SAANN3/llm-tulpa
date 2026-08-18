import { useEffect, useState, type ReactNode } from 'react'

import { ThemeContext } from './ThemeContext'
import { themeNames } from '../themes'

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [themeName, setThemeName] = useState<(typeof themeNames)[number]>(localStorage.getItem('theme_name') as (typeof themeNames)[number] ?? 'dark')

  // Exposes the active theme as `:root[data-theme="..."]` so each theme's CSS can
  // define its own color tokens there — that's what `variant` resolves against.
  // See ../components/THEMING.md before touching this or writing a theme's CSS — there
  // are collision rules around `data-theme`/`data-variant` that aren't obvious from this
  // file alone.
  useEffect(() => {
    localStorage.setItem('theme_name', themeName)
    document.documentElement.dataset.theme = themeName
  }, [themeName])

  return (
    <ThemeContext.Provider value={{ themeName, setThemeName, themeNames }}>
      {children}
    </ThemeContext.Provider>
  )
}
