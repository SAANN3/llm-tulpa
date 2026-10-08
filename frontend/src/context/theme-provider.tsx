import {useEffect, useState, type ReactNode} from 'react'
import {ThemeContext} from './theme-context.ts'
import {themeNames} from '../themes'
import {syncFaviconWithTheme} from '../utils/favicon.ts'

export const ThemeProvider = ({children}: { children: ReactNode }) => {
    const [themeName, setThemeName] = useState<(typeof themeNames)[number]>(localStorage.getItem('theme_name') as (typeof themeNames)[number] ?? 'dark')

    // Kept per browser like the theme, and on unless turned off
    const [dots, setDots] = useState(localStorage.getItem('background_dots') !== 'off')

    useEffect(() => {
        localStorage.setItem('theme_name', themeName)
        document.documentElement.dataset.theme = themeName
        syncFaviconWithTheme()
    }, [themeName])

    useEffect(() => {
        localStorage.setItem('background_dots', dots ? 'on' : 'off')
        document.documentElement.dataset.dots = dots ? 'on' : 'off'
    }, [dots])

    return (
        <ThemeContext.Provider value={{themeName, setThemeName, themeNames, dots, setDots}}>
            {children}
        </ThemeContext.Provider>
    )
};
