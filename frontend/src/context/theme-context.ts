import {createContext} from 'react'
import type {ThemeName} from '../themes'

export interface ThemeContextValue {
    themeName: ThemeName
    setThemeName: (name: ThemeName) => void
    themeNames: readonly ThemeName[]
    /** The accent-tinted dot grid behind the pages, like dot-grid paper */
    dots: boolean
    setDots: (dots: boolean) => void
}

export const ThemeContext = createContext<ThemeContextValue | null>(null)
