import {useEffect, useState, type ReactNode} from 'react'
import {flushSync} from 'react-dom'
import {ThemeContext} from './theme-context.ts'
import {DEFAULT_BACKGROUND, findBackground, type BackgroundSettings} from '../backgrounds'
import {themeNames} from '../themes'
import {syncFaviconWithTheme} from '../utils/favicon.ts'
import {revealTheme} from '../utils/theme-reveal.ts'
import {localeTimeFormat, type TimeFormat} from '../utils/time-format.ts'

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
    const [themeName, setThemeNameState] = useState<(typeof themeNames)[number]>(localStorage.getItem('theme_name') as (typeof themeNames)[number] ?? 'dark')
    // The attribute is set here as well as in the effect below: the reveal pictures the new page as soon as this
    // returns, before React would run the effect
    const setThemeName = (name: (typeof themeNames)[number], from?: Element | null) => revealTheme(from ?? null, () => {
        document.documentElement.dataset.theme = name
        flushSync(() => setThemeNameState(name))
    })

    // Kept per browser like the theme: a phone and a desktop may well want different ones
    const [background, setBackgroundState] = useState<BackgroundSettings>(loadBackground)
    const setBackground = (change: Partial<BackgroundSettings>) => setBackgroundState((now) => ({...now, ...change}))

    // Per browser too; until picked, whatever the browser's locale uses (saved only once picked, so a browser that
    // never picked follows its locale if that changes)
    const [timeFormat, setTimeFormatState] = useState<TimeFormat>(() => {
        const saved = localStorage.getItem('time_format')
        return saved === '24h' || saved === '12h' ? saved : localeTimeFormat()
    })
    const setTimeFormat = (format: TimeFormat) => {
        localStorage.setItem('time_format', format)
        setTimeFormatState(format)
    }

    // Per browser, off until picked
    const [sidebarSeeThrough, setSidebarSeeThroughState] = useState(() => localStorage.getItem('sidebar_see_through') === 'on')
    const setSidebarSeeThrough = (seeThrough: boolean) => {
        localStorage.setItem('sidebar_see_through', seeThrough ? 'on' : 'off')
        setSidebarSeeThroughState(seeThrough)
    }

    useEffect(() => {
        document.documentElement.dataset.sidebar = sidebarSeeThrough ? 'see-through' : 'solid'
    }, [sidebarSeeThrough])

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
        <ThemeContext.Provider value={{themeName, setThemeName, themeNames, background, setBackground, timeFormat, setTimeFormat, sidebarSeeThrough, setSidebarSeeThrough}}>
            {children}
        </ThemeContext.Provider>
    )
};
