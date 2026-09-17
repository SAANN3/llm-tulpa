import type { CSSProperties } from 'react'

import '../styles/ThemePreview.scss'
import { Div, Label, RadioButton } from './primitives'
import { useTheme } from '../context/useTheme'
import { themeDisplayNames } from '../themes'
import type { ThemeName } from '../themes'

/** A miniature, non-interactive rendering of the app's own layout (sidebar strip, an
 * accent row, a right-aligned "user" line, two muted "assistant" lines) — colored from
 * `theme`'s own tokens via a locally-scoped `data-theme`, independent of whichever
 * theme is actually active on the page. See `THEMING.md`'s "Safe: a theme's own
 * colors" section — the same `[data-theme="..."]` rule that colors the whole app off
 * `:root` colors this card off its own wrapper instead. */
function ThemeMiniature({ theme }: { theme: ThemeName }) {
  return (
    <div data-theme={theme} className="theme-picker__miniature">
      <div className="theme-picker__mini-sidebar">
        <div className="theme-picker__bar theme-picker__bar--accent" />
        <div className="theme-picker__bar theme-picker__bar--muted" />
        <div className="theme-picker__bar theme-picker__bar--muted" />
      </div>
      <div className="theme-picker__mini-content">
        <div className="theme-picker__bar theme-picker__bar--user" />
        <div className="theme-picker__bar theme-picker__bar--wide" />
        <div className="theme-picker__bar theme-picker__bar--narrow" />
      </div>
    </div>
  )
}

/** Three clickable theme cards — a miniature preview of each theme plus its name;
 * picking one applies it immediately via `setThemeName`. */
export function ThemePreview() {
  const { themeName, setThemeName, themeNames } = useTheme()

  return (
    <div className="theme-picker" style={{ '--theme-count': themeNames.length } as CSSProperties}>
      {themeNames.map((theme) => (
        <Div key={theme} className="theme-picker__card" onClick={() => setThemeName(theme)}>
          <ThemeMiniature theme={theme} />
          <Div className="theme-picker__label-row">
            <RadioButton name="theme-picker" value={theme} checked={themeName === theme} onChanged={() => setThemeName(theme)} />
            <Label className="theme-picker__name" text={themeDisplayNames[theme]} />
          </Div>
        </Div>
      ))}
    </div>
  )
}
