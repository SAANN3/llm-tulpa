import {createContext} from 'react'
import type {BackgroundSettings} from '../backgrounds'
import type {ThemeName} from '../themes'

export interface ThemeContextValue {
    themeName: ThemeName
    setThemeName: (name: ThemeName) => void
    themeNames: readonly ThemeName[]
    /** What is behind the pages: nothing, the static dots, or an animated background, with its brightness and speed */
    background: BackgroundSettings
    setBackground: (change: Partial<BackgroundSettings>) => void
}

export const ThemeContext = createContext<ThemeContextValue | null>(null)
