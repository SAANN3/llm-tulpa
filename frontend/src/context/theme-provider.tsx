import {useEffect, useState, type ReactNode} from 'react'
import {ThemeContext} from './theme-context.ts'
import {themeNames} from '../themes'

export const ThemeProvider = ({children}: { children: ReactNode }) => {
    const [themeName, setThemeName] = useState<(typeof themeNames)[number]>(localStorage.getItem('theme_name') as (typeof themeNames)[number] ?? 'dark')

    useEffect(() => {
        localStorage.setItem('theme_name', themeName)
        document.documentElement.dataset.theme = themeName
    }, [themeName])

    return (
        <ThemeContext.Provider value={{themeName, setThemeName, themeNames}}>
            {children}
        </ThemeContext.Provider>
    )
};
