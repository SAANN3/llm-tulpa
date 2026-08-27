import { Div, Label, RadioButton } from './primitives'
import { useTheme } from '../context/useTheme'
import { themeDisplayNames } from '../themes'
import type { ThemeName } from '../themes'

const CARD_HEIGHT = 66

/** A miniature, non-interactive rendering of the app's own layout (sidebar strip, an
 * accent row, a right-aligned "user" line, two muted "assistant" lines) — colored from
 * `theme`'s own tokens via a locally-scoped `data-theme`, independent of whichever
 * theme is actually active on the page. See `THEMING.md`'s "Safe: a theme's own
 * colors" section — the same `[data-theme="..."]` rule that colors the whole app off
 * `:root` colors this card off its own wrapper instead. */
function ThemeMiniature({ theme }: { theme: ThemeName }) {
  return (
    <div
      data-theme={theme}
      style={{
        display: 'flex',
        height: CARD_HEIGHT,
        width: '100%',
        border: '1px solid var(--color-border)',
        borderRadius: 7,
        overflow: 'hidden',
        background: 'var(--color-secondary)',
        color: 'var(--color-primary)',
      }}
    >
      <div
        style={{
          width: 26,
          background: 'var(--color-surface)',
          borderRight: '1px solid var(--color-border)',
          display: 'flex',
          flexDirection: 'column',
          gap: 3,
          padding: '5px 4px',
        }}
      >
        <div style={{ height: 5, borderRadius: 2, background: 'var(--color-tertiary)' }} />
        <div style={{ height: 3, borderRadius: 2, background: 'currentColor', opacity: 0.25 }} />
        <div style={{ height: 3, borderRadius: 2, background: 'currentColor', opacity: 0.25 }} />
      </div>
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 4, padding: '6px 5px' }}>
        <div style={{ alignSelf: 'flex-end', width: '70%', height: 4, borderRadius: 2, background: 'currentColor', opacity: 0.7 }} />
        <div style={{ width: '85%', height: 3, borderRadius: 2, background: 'currentColor', opacity: 0.3 }} />
        <div style={{ width: '60%', height: 3, borderRadius: 2, background: 'currentColor', opacity: 0.3 }} />
      </div>
    </div>
  )
}

/** Three clickable theme cards — a miniature preview of each theme plus its name;
 * picking one applies it immediately via `setThemeName`. */
export function ThemePreview() {
  const { themeName, setThemeName, themeNames } = useTheme()

  return (
    <Div className="vbox" style={{ gap: 10 }}>
      <div style={{ display: 'grid', gridTemplateColumns: `repeat(${themeNames.length}, 1fr)`, gap: 10 }}>
        {themeNames.map((theme) => (
          <Div
            key={theme}
            className="vbox"
            style={{ gap: 6, alignItems: 'center', cursor: 'pointer' }}
            onClick={() => setThemeName(theme)}
          >
            <ThemeMiniature theme={theme} />
            <Div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <RadioButton name="theme-picker" value={theme} checked={themeName === theme} onChanged={() => setThemeName(theme)} />
              <Label text={themeDisplayNames[theme]} style={{ fontSize: 13 }} />
            </Div>
          </Div>
        ))}
      </div>
    </Div>
  )
}
