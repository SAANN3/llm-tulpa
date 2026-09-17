import {useContext} from 'react'
import {ThemeContext, type ThemeContextValue} from './theme-context.ts'

/** The active theme name, its setter, and the list of known theme names */
export const useTheme = (): ThemeContextValue => {
    const context = useContext(ThemeContext)

    if (!context) {
        throw new Error('useTheme must be used within a ThemeProvider')
    }

    return context
};
