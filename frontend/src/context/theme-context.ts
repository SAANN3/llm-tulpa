import {createContext} from 'react'
import type {BackgroundSettings} from '../backgrounds'
import type {ThemeName} from '../themes'
import type {TimeFormat} from '../utils/time-format.ts'

export interface ThemeContextValue {
    themeName: ThemeName
    setThemeName: (name: ThemeName) => void
    themeNames: readonly ThemeName[]
    /** What is behind the pages: nothing, the static dots, or an animated background, with its brightness and speed */
    background: BackgroundSettings
    setBackground: (change: Partial<BackgroundSettings>) => void
    /** How times of day are shown: 24-hour or 12-hour */
    timeFormat: TimeFormat
    setTimeFormat: (format: TimeFormat) => void
    /** The sidebar lets the background show faintly through it instead of covering it */
    sidebarSeeThrough: boolean
    setSidebarSeeThrough: (seeThrough: boolean) => void
}

export const ThemeContext = createContext<ThemeContextValue | null>(null)
