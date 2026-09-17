import { Loader } from 'pixelarticons/react'

export interface MarkProps {
  /** Rotates while true (loading), settles static at rest otherwise — see `[data-mark]` in `themes/variants.scss`. */
  spinning: boolean
  size?: number
}

/** The app's home mark / loading indicator — a pixelarticons `Loader` glyph, colored
 * from the active theme via `currentColor`. Spins while `spinning`, sits still
 * otherwise (see `[data-mark]`/`mark-spin` in `themes/variants.scss`). */
export function Mark({ spinning, size = 60 }: MarkProps) {
  return (
    <span data-mark data-spinning={spinning} style={{ color: 'var(--color-primary)', display: 'inline-flex' }}>
      <Loader width={size} height={size} />
    </span>
  )
}
