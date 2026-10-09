import '../styles/variants.scss'

import './dark.scss'
import './white.scss'
import './matcha-dark.scss'
import './ember.scss'
import './twilight.scss'
import './cinder.scss'
import './cozy.scss'
import './cozy-night.scss'
import './cocoa.scss'
import './hearth.scss'
import './oatmeal.scss'
import './chill.scss'
import './chill-day.scss'
import './lagoon.scss'
import './fog.scss'
import './dusk.scss'
import './moss.scss'
import './sakura.scss'
import './rose-dusk.scss'
import './ocean.scss'
import './dune.scss'
import './ink.scss'
import './amber.scss'
import './phosphor.scss'
import './high-contrast.scss'

// Each name has a `<name>.scss` here providing that theme's `[data-theme="<name>"]` colors
// (see THEMING.md).
export const themeNames = [
    'dark', 'white', 'matcha-dark', 'ember', 'twilight', 'cinder',
    'cozy', 'cozy-night', 'cocoa', 'hearth', 'oatmeal',
    'chill', 'chill-day', 'lagoon', 'fog', 'dusk',
    'moss', 'sakura', 'rose-dusk', 'ocean', 'dune', 'ink', 'amber', 'phosphor', 'high-contrast',
] as const

export type ThemeName = (typeof themeNames)[number]

export const themeDisplayNames: Record<ThemeName, string> = {
    'matcha-dark': 'Matcha',
    white: 'Paper',
    dark: 'Slate',
    ember: 'Ember',
    twilight: 'Twilight',
    cinder: 'Cinder',
    cozy: 'Cozy',
    'cozy-night': 'Cozy Night',
    cocoa: 'Cocoa',
    hearth: 'Hearth',
    oatmeal: 'Oatmeal',
    chill: 'Chill',
    'chill-day': 'Chill Day',
    lagoon: 'Lagoon',
    fog: 'Fog',
    dusk: 'Dusk',
    moss: 'Moss',
    sakura: 'Sakura',
    'rose-dusk': 'Rose Dusk',
    ocean: 'Ocean',
    dune: 'Dune',
    ink: 'Ink',
    amber: 'Amber',
    phosphor: 'Phosphor',
    'high-contrast': 'High Contrast',
}

/** The themes as the picker lists them, by kind; every name in `themeNames` is in exactly one group */
export const themeGroups: readonly { name: string; themes: readonly ThemeName[] }[] = [
    {name: 'Classic', themes: ['dark', 'white', 'matcha-dark', 'ember', 'twilight', 'cinder']},
    {name: 'Cozy', themes: ['cozy', 'cozy-night', 'cocoa', 'hearth', 'oatmeal']},
    {name: 'Chill', themes: ['chill', 'chill-day', 'lagoon', 'fog', 'dusk']},
    {name: 'Nature', themes: ['moss', 'sakura', 'rose-dusk', 'ocean', 'dune', 'ink']},
    {name: 'Terminal', themes: ['amber', 'phosphor', 'high-contrast']},
]
