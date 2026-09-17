import '../styles/variants.scss'

import '../styles/dark.scss'
import '../styles/white.scss'
import '../styles/matcha-dark.scss'

export const themeNames = ['dark', 'white', 'matcha-dark'] as const

export type ThemeName = (typeof themeNames)[number]

export const themeDisplayNames: Record<ThemeName, string> = {
    'matcha-dark': 'Matcha',
    white: 'Paper',
    dark: 'Slate',
}
