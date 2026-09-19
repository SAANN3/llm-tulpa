import '../styles/variants.scss'

import './dark.scss'
import './white.scss'
import './matcha-dark.scss'

// Each name has a `<name>.scss` here providing that theme's `[data-theme="<name>"]` colors
// (see THEMING.md).
export const themeNames = ['dark', 'white', 'matcha-dark'] as const

export type ThemeName = (typeof themeNames)[number]

export const themeDisplayNames: Record<ThemeName, string> = {
    'matcha-dark': 'Matcha',
    white: 'Paper',
    dark: 'Slate',
}
