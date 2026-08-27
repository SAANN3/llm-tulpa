export interface MarkProps {
  /** Rotates while true (loading), settles static at rest otherwise — see `[data-mark]` in `themes/variants.css`. */
  spinning: boolean
  size?: number
}

/** The app's favicon, inlined as SVG (not `<img>`) so its petals pick up `currentColor` — the same four-petal pinwheel used everywhere else, just recolored from whatever theme is active instead of its own hardcoded fill. Doubles as Home's loading indicator: spins while `spinning`, sits still otherwise. */
export function Mark({ spinning, size = 60 }: MarkProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      data-mark
      data-spinning={spinning}
      style={{ color: 'var(--color-primary)' }}
    >
      <g fill="currentColor" transform="translate(16,16)">
        <path d="M0,0 C3,-7 9,-5 12,0 C8,1 3,4 0,0 Z" />
        <path d="M0,0 C3,-7 9,-5 12,0 C8,1 3,4 0,0 Z" transform="rotate(90)" />
        <path d="M0,0 C3,-7 9,-5 12,0 C8,1 3,4 0,0 Z" transform="rotate(180)" />
        <path d="M0,0 C3,-7 9,-5 12,0 C8,1 3,4 0,0 Z" transform="rotate(270)" />
      </g>
    </svg>
  )
}
