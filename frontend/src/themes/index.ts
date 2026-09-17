import '../styles/variants.scss'

import '../styles/dark.scss'
import '../styles/white.scss'
import '../styles/matcha-dark.scss'
/**
 * Known theme names — each corresponds to a `themes/<name>/` CSS file providing that
 * theme's `:root[data-theme="<name>"]` color tokens (see `../THEMING.md`). Plain
 * identifiers, not necessarily human-friendly — add display labels/descriptions here if
 * that's ever needed.
 */
export const themeNames = ['dark', 'white', 'matcha-dark'] as const

export type ThemeName = (typeof themeNames)[number]

/** Human-facing name for each theme, shown on its picker card. */
export const themeDisplayNames: Record<ThemeName, string> = {
  'matcha-dark': 'Matcha',
  white: 'Paper',
  dark: 'Slate',
}
