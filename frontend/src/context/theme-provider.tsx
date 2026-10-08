import {useEffect, useState, type ReactNode} from 'react'
import {ThemeContext} from './theme-context.ts'
import {DEFAULT_BACKGROUND, findBackground, type BackgroundSettings} from '../backgrounds'
import {themeNames} from '../themes'
import {syncFaviconWithTheme} from '../utils/favicon.ts'

/** The saved background, or the default. A browser that only has the older dots switch keeps its choice. */
const loadBackground = (): BackgroundSettings => {
    try {
        const saved = JSON.parse(localStorage.getItem('background') ?? 'null') as Partial<BackgroundSettings> | null
        if (saved && (saved.id === 'off' || saved.id === 'dots' || (saved.id && findBackground(saved.id)))) return {...DEFAULT_BACKGROUND, ...saved}
    } catch {
        // A damaged value: the default instead
    }
    return {...DEFAULT_BACKGROUND, id: localStorage.getItem('background_dots') === 'off' ? 'off' : 'dots'}
}

export const ThemeProvider = ({children}: { children: ReactNode }) => {
    const [themeName, setThemeName] = useState<(typeof themeNames)[number]>(localStorage.getItem('theme_name') as (typeof themeNames)[number] ?? 'dark')

    // Kept per browser like the theme: a phone and a desktop may well want different ones
    const [background, setBackgroundState] = useState<BackgroundSettings>(loadBackground)
    const setBackground = (change: Partial<BackgroundSettings>) => setBackgroundState((now) => ({...now, ...change}))

    useEffect(() => {
        localStorage.setItem('theme_name', themeName)
        document.documentElement.dataset.theme = themeName
        syncFaviconWithTheme()
    }, [themeName])

    useEffect(() => {
        localStorage.setItem('background', JSON.stringify(background))
        localStorage.removeItem('background_dots')
        document.documentElement.dataset.dots = background.id === 'dots' ? 'on' : 'off'
    }, [background])

    return (
        <ThemeContext.Provider value={{themeName, setThemeName, themeNames, background, setBackground}}>
            {children}
        </ThemeContext.Provider>
    )
};
